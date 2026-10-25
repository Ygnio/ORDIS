//! Optional local HTTP embeddings. Never starts a process or downloads a model.
use super::*;
use std::io::Read;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub enabled: bool,
    pub endpoint: String,
    pub model_id: String,
    pub model_path: String,
    pub dimension: usize,
    pub query_prefix: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: "http://127.0.0.1:8081".into(),
            model_id: String::new(),
            model_path: String::new(),
            dimension: 1024,
            query_prefix:
                "Instruct: Given a search query, retrieve relevant personal memories.\nQuery: "
                    .into(),
        }
    }
}
fn validate(c: Config) -> Result<Config, String> {
    let u = url::Url::parse(&c.endpoint).map_err(|_| "Endpoint local inválido.")?;
    if u.scheme() != "http"
        || !matches!(u.host_str(), Some("127.0.0.1" | "[::1]"))
        || !u.username().is_empty()
        || u.password().is_some()
        || u.query().is_some()
        || u.fragment().is_some()
        || u.path() != "/"
        || !(1..=4096).contains(&c.dimension)
        || c.model_id.len() > 200
        || c.model_path.len() > 4096
        || c.query_prefix.len() > 512
        || (c.enabled && c.model_id.trim().is_empty())
    {
        return Err("Use HTTP loopback literal, modelo explícito e dimensão 1–4096.".into());
    }
    Ok(c)
}
pub(super) fn config(s: &Store) -> Result<Config, String> {
    let raw: Option<String> =
        s.db.query_row("SELECT config FROM semantic_settings WHERE id=1", [], |r| {
            r.get(0)
        })
        .optional()
        .map_err(error)?;
    raw.map(|r| serde_json::from_str(&r).map_err(|_| "Configuração semântica inválida.".into()))
        .unwrap_or(Ok(Config::default()))
        .and_then(validate)
}
fn unit(mut v: Vec<f32>, dimension: usize) -> Result<Vec<f32>, String> {
    if v.len() != dimension || v.iter().any(|x| !x.is_finite()) {
        return Err("embedding_dimension_or_nonfinite".into());
    }
    let norm = v.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    if norm < 1e-12 {
        return Err("embedding_zero_vector".into());
    }
    for x in &mut v {
        *x = (*x as f64 / norm) as f32;
    }
    Ok(v)
}
pub(super) fn embed(c: &Config, text: &str, query: bool) -> Result<Vec<f32>, String> {
    validate(c.clone())?;
    if !c.enabled {
        return Err("embedding_disabled".into());
    }
    let input = if query {
        format!("{}{text}", c.query_prefix)
    } else {
        text.into()
    };
    // No DNS, proxy, redirects, authentication, remote URLs or response bodies in errors.
    let agent = ureq::AgentBuilder::new()
        .try_proxy_from_env(false)
        .redirects(0)
        .timeout(Duration::from_secs(3))
        .build();
    let response = agent
        .post(&format!(
            "{}/v1/embeddings",
            c.endpoint.trim_end_matches('/')
        ))
        .send_json(json!({"model":c.model_id,"input":input,"encoding_format":"float"}))
        .map_err(|_| "embedding_provider_unavailable")?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(262145)
        .read_to_end(&mut bytes)
        .map_err(|_| "embedding_response_invalid")?;
    if bytes.len() > 262144 {
        return Err("embedding_response_oversized".into());
    }
    let v: Value = serde_json::from_slice(&bytes).map_err(|_| "embedding_response_invalid")?;
    let values = v["data"][0]["embedding"]
        .as_array()
        .ok_or("embedding_response_invalid")?;
    unit(
        values
            .iter()
            .map(|x| {
                x.as_f64()
                    .map(|x| x as f32)
                    .ok_or("embedding_response_invalid")
            })
            .collect::<Result<Vec<_>, _>>()?,
        c.dimension,
    )
}
/// Snapshot under a short lock, inference outside the SQLite lock, conditional commit.
pub fn tick(c: &Caminhos, cancel: &AtomicBool) -> Result<Value, String> {
    if cancel.load(Ordering::Relaxed) || ACTIVE_CHATS.load(Ordering::SeqCst) > 0 {
        return Ok(json!({"paused":true}));
    }
    let snapshot = with_chat_store(c, |s| {
        active::expire(s)?;
        let cfg = config(s)?;
        if !cfg.enabled {
            return Ok(None);
        }
        let row=s.db.query_row("SELECT m.id,m.content,q.revision FROM semantic_queue q JOIN memories m ON m.id=q.memory_id WHERE q.state IN ('pending','error') AND q.retry_at<=?1 AND m.state='active' ORDER BY q.attempts,m.id LIMIT 1",[now()],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?))).optional().map_err(error)?;
        Ok(row.map(|r| (cfg, r)))
    })?;
    let Some((cfg, (id, content, revision))) = snapshot else {
        return Ok(json!({"idle":true}));
    };
    if has_secret(&content) || private_for_memory(&content) {
        return with_chat_store(c, |s| {
            s.db.execute(
                "UPDATE semantic_queue SET state='excluded' WHERE memory_id=?1 AND revision=?2",
                params![id, revision],
            )
            .map_err(error)?;
            Ok(json!({"excluded":true}))
        });
    }
    let started = Instant::now();
    let result = embed(&cfg, &content, false);
    if cancel.load(Ordering::Relaxed) || ACTIVE_CHATS.load(Ordering::SeqCst) > 0 {
        return Ok(json!({"cancelled":true}));
    }
    with_chat_store(c, |s| {
        if config(s)? != cfg {
            return Ok(json!({"stale":true}));
        }
        let current:bool=s.db.query_row("SELECT EXISTS(SELECT 1 FROM semantic_queue q JOIN memories m ON m.id=q.memory_id WHERE m.id=?1 AND m.state='active' AND q.revision=?2 AND m.content=?3)",params![id,revision,content],|r|r.get(0)).map_err(error)?;
        if !current {
            return Ok(json!({"stale":true}));
        }
        match result {
            Ok(vector) => {
                s.db.execute_batch("SAVEPOINT semantic_commit")
                    .map_err(error)?;
                let result = (|| {
                    s.db.execute(
                        "INSERT OR REPLACE INTO semantic_vectors VALUES(?1,?2,?3,?4,?5,?6)",
                        params![
                            id,
                            revision,
                            cfg.model_id,
                            cfg.dimension as i64,
                            now(),
                            json!(vector).to_string()
                        ],
                    )
                    .map_err(error)?;
                    s.db.execute("UPDATE semantic_queue SET state='ready',error=NULL,attempts=0 WHERE memory_id=?1",[id]).map_err(error)?;
                    Ok(json!({"indexed":1,"elapsed_ms":started.elapsed().as_secs_f64()*1000.0}))
                })();
                if result.is_ok() {
                    s.db.execute_batch("RELEASE semantic_commit")
                        .map_err(error)?;
                } else {
                    s.db.execute_batch("ROLLBACK TO semantic_commit; RELEASE semantic_commit")
                        .map_err(error)?;
                }
                result
            }
            Err(code) => {
                s.db.execute("UPDATE semantic_queue SET state='error',attempts=attempts+1,error=?2,retry_at=?3 WHERE memory_id=?1",params![id,code,now()+30]).map_err(error)?;
                Ok(json!({"indexed":0,"error":code}))
            }
        }
    })
}
/// Exact cosine scan, bounded to 10,000 rows; only one vector is decoded at a time.
/// SQLite remains authoritative for scope, approval, expiry and current revision.
pub(super) fn ranked(
    s: &Store,
    query: &str,
    project: Option<i64>,
    category: Option<&str>,
    hybrid: bool,
) -> Result<Vec<Value>, String> {
    active::expire(s)?;
    let mut q=s.db.prepare("SELECT m.id,m.kind,m.content,m.provenance,m.project_id FROM memories_fts JOIN memories m ON m.id=memories_fts.rowid LEFT JOIN memory_labels l ON l.memory_id=m.id WHERE memories_fts MATCH ?1 AND m.state='active' AND (m.project_id IS NULL OR m.project_id IS ?2) AND (?3 IS NULL OR coalesce(l.category,CASE m.kind WHEN 'preference' THEN 'preferences' WHEN 'hardware' THEN 'technical_profile' WHEN 'project' THEN 'projects' WHEN 'decision' THEN 'decisions' WHEN 'goal' THEN 'tasks' WHEN 'task' THEN 'tasks' WHEN 'solution' THEN 'knowledge' WHEN 'summary' THEN 'experiences' WHEN 'constraint' THEN 'instructions' ELSE 'user_identity' END)=?3) ORDER BY (bm25(memories_fts)-m.importance*0.25-CASE WHEN m.project_id IS ?2 AND ?2 IS NOT NULL THEN 2 ELSE 0 END-CASE WHEN m.kind IN ('constraint','decision','preference') THEN 0.5 ELSE 0 END-CASE WHEN m.provenance IN ('user','manual') THEN 0.5 ELSE 0 END-1.0/(1.0+max(0,?4-m.updated_at)/86400.0)) ASC LIMIT 100").map_err(error)?;
    let fts = search_terms(query);
    let lexical = if fts.is_empty() {
        vec![]
    } else {
        q.query_map(params![fts,project,category,now()],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"type":r.get::<_,String>(1)?,"data":r.get::<_,String>(2)?,"origin":r.get::<_,String>(3)?,"project":r.get::<_,Option<i64>>(4)?}))).map_err(error)?.collect::<Result<Vec<_>,_>>().map_err(error)?
    };
    let cfg = config(s)?;
    let mut vector = Vec::new();
    if hybrid && cfg.enabled && !has_secret(query) && !private_for_memory(query) {
        // One bounded local query (3s deadline). Provider failure never disables lexical search.
        if let Ok(embedding) = embed(&cfg, query, true) {
            let mut q=s.db.prepare("SELECT m.id,m.kind,m.content,m.provenance,m.project_id,v.vector FROM semantic_vectors v JOIN memories m ON m.id=v.memory_id JOIN semantic_queue q ON q.memory_id=m.id LEFT JOIN memory_labels l ON l.memory_id=m.id WHERE m.state='active' AND q.state='ready' AND q.revision=v.revision AND v.model_id=?1 AND v.dimension=?2 AND (m.project_id IS NULL OR m.project_id IS ?3) AND (?4 IS NULL OR coalesce(l.category,CASE m.kind WHEN 'preference' THEN 'preferences' WHEN 'hardware' THEN 'technical_profile' WHEN 'project' THEN 'projects' WHEN 'decision' THEN 'decisions' WHEN 'goal' THEN 'tasks' WHEN 'task' THEN 'tasks' WHEN 'solution' THEN 'knowledge' WHEN 'summary' THEN 'experiences' WHEN 'constraint' THEN 'instructions' ELSE 'user_identity' END)=?4) ORDER BY m.id LIMIT 10000").map_err(error)?;
            let rows=q.query_map(params![cfg.model_id,cfg.dimension as i64,project,category],|r|Ok((json!({"id":r.get::<_,i64>(0)?,"type":r.get::<_,String>(1)?,"data":r.get::<_,String>(2)?,"origin":r.get::<_,String>(3)?,"project":r.get::<_,Option<i64>>(4)?}),r.get::<_,String>(5)?))).map_err(error)?;
            for row in rows {
                let (item, raw) = row.map_err(error)?;
                if let Ok(v) = serde_json::from_str::<Vec<f32>>(&raw) {
                    if let Ok(v) = unit(v, cfg.dimension) {
                        let score = v.iter().zip(&embedding).map(|(a, b)| a * b).sum::<f32>();
                        if score > 0.25 {
                            vector.push((item, score));
                        }
                    }
                }
            }
            vector.sort_by(|a, b| b.1.total_cmp(&a.1));
            vector.truncate(100);
        }
    }
    let mut fused: BTreeMap<i64, (Value, f64, bool, bool)> = BTreeMap::new();
    for (semantic, rows) in [
        (false, lexical),
        (true, vector.into_iter().map(|(v, _)| v).collect()),
    ] {
        for (rank, row) in rows.into_iter().enumerate() {
            let id = row["id"].as_i64().unwrap();
            let e = fused.entry(id).or_insert((row, 0.0, false, false));
            e.1 += 1.0 / (60.0 + rank as f64 + 1.0);
            if semantic {
                e.3 = true
            } else {
                e.2 = true
            }
        }
    }
    let mut rows: Vec<_> = fused.into_values().collect();
    rows.sort_by(|a, b| {
        b.1.total_cmp(&a.1)
            .then_with(|| a.0["id"].as_i64().cmp(&b.0["id"].as_i64()))
    });
    Ok(rows
        .into_iter()
        .map(|(mut row, score, l, v)| {
            row["retrieval"] = json!(if l && v {
                "both"
            } else if v {
                "vector"
            } else {
                "lexical"
            });
            row["rrf"] = json!(score);
            row
        })
        .collect())
}
pub(super) fn action(s: &Store, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "semantic_get" => {
            let cfg = config(s)?;
            let mut q =
                s.db.prepare("SELECT state,count(*),max(error) FROM semantic_queue GROUP BY state")
                    .map_err(error)?;
            let states=q.query_map([],|r|Ok(json!({"state":r.get::<_,String>(0)?,"count":r.get::<_,i64>(1)?,"error":r.get::<_,Option<String>>(2)?}))).map_err(error)?.collect::<Result<Vec<_>,_>>().map_err(error)?;
            Ok(
                json!({"config":cfg,"index":states,"provider":"local_http","strategy":"exact_cosine_rrf60","scan_limit":10000}),
            )
        }
        "semantic_save" => {
            let cfg = validate(
                serde_json::from_value(args.clone()).map_err(|_| "Configuração inválida.")?,
            )?;
            let old = config(s)?;
            s.db.execute_batch("SAVEPOINT semantic_config")
                .map_err(error)?;
            let result = (|| {
                s.db.execute(
                    "INSERT OR REPLACE INTO semantic_settings VALUES(1,?1)",
                    [json!(cfg).to_string()],
                )
                .map_err(error)?;
                if old.model_id != cfg.model_id
                    || old.dimension != cfg.dimension
                    || old.model_path != cfg.model_path
                    || old.endpoint != cfg.endpoint
                {
                    reindex(s)?;
                }
                Ok(json!(true))
            })();
            if result.is_ok() {
                s.db.execute_batch("RELEASE semantic_config")
                    .map_err(error)?;
            } else {
                s.db.execute_batch("ROLLBACK TO semantic_config; RELEASE semantic_config")
                    .map_err(error)?;
            }
            result
        }
        "semantic_reindex" => {
            reindex(s)?;
            Ok(json!(true))
        }
        "semantic_compare" => {
            let query = args["query"].as_str().ok_or("Consulta inválida.")?;
            if query.len() > 1000 || private_for_memory(query) || has_secret(query) {
                return Err("Consulta privada ou extensa.".into());
            }
            let project = args["project_id"].as_i64();
            let cat = args["category"].as_str();
            let start = Instant::now();
            let lexical = ranked(s, query, project, cat, false)?;
            let hybrid = ranked(s, query, project, cat, true)?;
            Ok(
                json!({"lexical":lexical.into_iter().take(20).collect::<Vec<_>>(),"hybrid":hybrid.into_iter().take(20).collect::<Vec<_>>(),"elapsed_ms":start.elapsed().as_secs_f64()*1000.0}),
            )
        }
        _ => Err("Ação semântica inválida.".into()),
    }
}
fn reindex(s: &Store) -> Result<(), String> {
    s.db.execute_batch("DELETE FROM semantic_vectors; INSERT INTO semantic_queue(memory_id) SELECT id FROM memories WHERE state='active' ON CONFLICT(memory_id) DO UPDATE SET revision=revision+1,state='pending',attempts=0,error=NULL,retry_at=0;").map_err(error)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vectors_validate() {
        assert!(unit(vec![1.0], 2).is_err());
        assert!(unit(vec![f32::NAN], 1).is_err());
        assert!(unit(vec![0.0], 1).is_err());
        assert_eq!(unit(vec![2.0, 0.0], 2).unwrap(), vec![1.0, 0.0]);
    }
    #[test]
    fn local_only() {
        let mut c = Config::default();
        c.endpoint = "http://example.com".into();
        assert!(validate(c).is_err());
        let mut c = Config::default();
        c.endpoint = "http://127.0.0.1:8081".into();
        assert!(validate(c).is_ok());
    }
    fn store() -> Store {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA trusted_schema=OFF;")
            .unwrap();
        for sql in MIGRATIONS {
            db.execute_batch(sql).unwrap();
        }
        Store {
            db,
            path: PathBuf::from("synthetic-only"),
        }
    }
    fn memory(s: &mut Store, text: &str, project: Option<i64>) -> i64 {
        s.put(
            None,
            &Input {
                kind: "solution".into(),
                content: text.into(),
                project_id: project,
                fact_key: String::new(),
                state: "active".into(),
                importance: 3,
            },
            "manual",
            None,
        )
        .unwrap()
    }
    fn mock(values: Value) -> (String, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut buf = [0; 32768];
            let _ = stream.read(&mut buf).unwrap();
            let body = json!({"data":[{"embedding":values}]}).to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        });
        (endpoint, handle)
    }
    fn configure(s: &Store, endpoint: &str) {
        action(
            s,
            "semantic_save",
            &json!(Config {
                enabled: true,
                endpoint: endpoint.into(),
                model_id: "synthetic-v1".into(),
                dimension: 2,
                ..Config::default()
            }),
        )
        .unwrap();
    }
    fn vector(s: &Store, id: i64) {
        let rev: i64 =
            s.db.query_row(
                "SELECT revision FROM semantic_queue WHERE memory_id=?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        s.db.execute(
            "INSERT INTO semantic_vectors VALUES(?1,?2,'synthetic-v1',2,1,'[1,0]')",
            params![id, rev],
        )
        .unwrap();
        s.db.execute(
            "UPDATE semantic_queue SET state='ready' WHERE memory_id=?1",
            [id],
        )
        .unwrap();
    }
    #[test]
    fn semantic_matches_without_lexical_and_deletion_is_immediate() {
        let mut s = store();
        let id = memory(
            &mut s,
            "Prefiro criação de personagem e evolução de classes.",
            None,
        );
        let (endpoint, server) = mock(json!([1, 0]));
        configure(&s, &endpoint);
        vector(&s, id);
        assert!(ranked(&s, "RPG", None, None, false).unwrap().is_empty());
        let rows = ranked(&s, "RPG", None, None, true).unwrap();
        assert_eq!(rows[0]["id"], id);
        assert_eq!(rows[0]["retrieval"], "vector");
        server.join().unwrap();
        s.delete(id).unwrap();
        assert_eq!(
            s.db.query_row("SELECT count(*) FROM semantic_vectors", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn bad_dimension_and_provider_failure_fall_back_to_fts() {
        let mut s = store();
        memory(&mut s, "Synthetic lexical dragon", None);
        let (endpoint, server) = mock(json!([1, 0, 1]));
        configure(&s, &endpoint);
        assert_eq!(ranked(&s, "dragon", None, None, true).unwrap().len(), 1);
        server.join().unwrap();
        configure(&s, "http://127.0.0.1:1");
        assert_eq!(ranked(&s, "dragon", None, None, true).unwrap().len(), 1);
    }
    #[test]
    fn model_change_reindex_and_update_invalidate() {
        let mut s = store();
        let id = memory(&mut s, "Synthetic indexed dragon", None);
        configure(&s, "http://127.0.0.1:1");
        vector(&s, id);
        let mut cfg = config(&s).unwrap();
        cfg.model_id = "synthetic-v2".into();
        action(&s, "semantic_save", &json!(cfg)).unwrap();
        assert_eq!(
            s.db.query_row("SELECT count(*) FROM semantic_vectors", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        let rev: i64 =
            s.db.query_row("SELECT revision FROM semantic_queue", [], |r| r.get(0))
                .unwrap();
        s.db.execute(
            "UPDATE memories SET content='Synthetic changed dragon' WHERE id=?1",
            [id],
        )
        .unwrap();
        assert!(
            s.db.query_row("SELECT revision FROM semantic_queue", [], |r| r
                .get::<_, i64>(0))
                .unwrap()
                > rev
        );
    }
    #[test]
    fn missing_index_and_scope_are_safe() {
        let mut s = store();
        memory(&mut s, "Synthetic global dragon", None);
        s.db.execute(
            "INSERT INTO projects(id,name,updated_at)VALUES(1,'Synthetic project',1)",
            [],
        )
        .unwrap();
        memory(&mut s, "Synthetic private project dragon", Some(1));
        assert_eq!(ranked(&s, "dragon", None, None, true).unwrap().len(), 1);
        assert_eq!(ranked(&s, "dragon", Some(1), None, true).unwrap().len(), 2);
        assert_eq!(
            ranked(&s, "dragon", None, Some("knowledge"), true)
                .unwrap()
                .len(),
            1
        );
        assert!(ranked(&s, "dragon", None, Some("tasks"), true)
            .unwrap()
            .is_empty());
    }
}
