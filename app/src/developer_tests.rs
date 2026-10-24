use super::*;
struct Fixture {
    c: Caminhos,
    root: PathBuf,
    home: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let id = crate::controle::sufixo_unico();
        let home = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap())
            .join("Lucille/ORDIS-Teste")
            .join(format!("developer-synthetic-{id}"));
        let root = std::env::temp_dir().join(format!("developer-workspace-synthetic-{id}"));
        fs::create_dir_all(&root).unwrap();
        let c = Caminhos {
            projeto: home.clone(),
            runtime: home.join("runtime"),
        };
        let mut cfg = Config::default();
        cfg.permissions.workspace = root.to_string_lossy().into();
        cfg.permissions.enabled = true;
        cfg.permissions.allow_read = true;
        cfg.permissions.allow_write = true;
        action(&c, "save", json!(cfg)).unwrap();
        Self { c, root, home }
    }
    fn call(&self, name: &str, args: Value) -> Result<Value, String> {
        execute_with(
            &self.c,
            "synthetic",
            name,
            args,
            &AtomicBool::new(false),
            &|_| {},
            &|_, _| Ok(true),
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
        let _ = fs::remove_dir_all(&self.home);
    }
}
fn patch(path: &str, old: &str, new: &str) -> Value {
    json!({"path":path,"old":old,"new":new,"expected_sha256":hash(old.as_bytes()),"reason":"Synthetic correction"})
}
#[test]
fn unicode_write_read_patch_move_delete_and_mkdir() {
    let f = Fixture::new();
    f.call(
        "workspace_write",
        json!({"path":"ação.py","content":"print(1)","reason":"Synthetic file"}),
    )
    .unwrap();
    let read = f.call("workspace_read", json!({"path":"ação.py"})).unwrap();
    assert_eq!(read["lines"][0]["text"], "print(1)");
    f.call("workspace_patch", patch("ação.py", "print(1)", "print(2)"))
        .unwrap();
    assert_eq!(
        fs::read_to_string(f.root.join("ação.py")).unwrap(),
        "print(2)"
    );
    f.call(
        "workspace_move",
        json!({"path":"ação.py","destination":"renamed.py","reason":"Synthetic rename"}),
    )
    .unwrap();
    f.call(
        "workspace_delete",
        json!({"path":"renamed.py","reason":"Synthetic removal"}),
    )
    .unwrap();
    f.call(
        "workspace_mkdir",
        json!({"path":"new","reason":"Synthetic directory"}),
    )
    .unwrap();
    f.call(
        "workspace_delete",
        json!({"path":"new","reason":"Synthetic removal"}),
    )
    .unwrap();
    assert!(!f.root.join("renamed.py").exists());
}
#[test]
fn refusal_changes_nothing_and_blocks_retry_this_turn() {
    let f = Fixture::new();
    let task = crate::controle::sufixo_unico();
    let args = json!({"path":"a.py","content":"print(1)","reason":"Synthetic"});
    let v = execute_with(
        &f.c,
        &task,
        "workspace_write",
        args.clone(),
        &AtomicBool::new(false),
        &|_| {},
        &|_, _| Ok(false),
    )
    .unwrap();
    assert_eq!(v["state"], "refused");
    assert!(!f.root.join("a.py").exists());
    assert!(execute_with(
        &f.c,
        &task,
        "workspace_write",
        args,
        &AtomicBool::new(false),
        &|_| {},
        &|_, _| panic!("must not ask again")
    )
    .is_err());
    begin(&f.c, &task, "Synthetic resumed by user");
    assert!(!REFUSED.lock().unwrap().contains(&task));
}
#[test]
fn conflict_after_approval_and_cancellation_do_not_overwrite() {
    let f = Fixture::new();
    fs::write(f.root.join("a.py"), "old").unwrap();
    assert!(execute_with(
        &f.c,
        "conflict",
        "workspace_patch",
        patch("a.py", "old", "new"),
        &AtomicBool::new(false),
        &|_| {},
        &|_, _| {
            fs::write(f.root.join("a.py"), "external").unwrap();
            Ok(true)
        }
    )
    .is_err());
    assert_eq!(fs::read_to_string(f.root.join("a.py")).unwrap(), "external");
    let cancel = AtomicBool::new(true);
    assert!(execute(
        &f.c,
        "cancel",
        "workspace_read",
        json!({"path":"a.py"}),
        &cancel,
        &|_| {}
    )
    .is_err());
}
#[test]
fn path_binary_missing_hardlink_and_private_guards() {
    let f = Fixture::new();
    let scope = Scope::new(&f.c, &load(&f.c)).unwrap();
    for path in [
        "../outside",
        "C:\\Windows\\win.ini",
        ".git/config",
        ".env",
        "NUL",
        "a.txt:stream",
        "missing",
        "..\\x",
    ] {
        assert!(scope.path(path, false).is_err(), "{path}");
    }
    fs::write(f.root.join("binary"), [0, 1, 2]).unwrap();
    assert!(f.call("workspace_read", json!({"path":"binary"})).is_err());
    fs::write(f.root.join("source"), "synthetic").unwrap();
    fs::hard_link(f.root.join("source"), f.root.join("link")).unwrap();
    assert!(scope.bytes("link").is_err());
    assert!(f
        .call("workspace_read", json!({"path":"source","unexpected":true}))
        .is_err());
    assert!(f.call("workspace_patch", json!({"path":"source"})).is_err());
    assert!(parse::<ReadArgs>(json!("malformed")).is_err());
}
#[test]
fn junction_escape_blocked_without_following_or_deleting_target() {
    let f = Fixture::new();
    let outside = std::env::temp_dir().join(format!(
        "developer-junction-synthetic-{}",
        crate::controle::sufixo_unico()
    ));
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("safe.txt"), "synthetic").unwrap();
    use std::os::windows::process::CommandExt;
    let status = std::process::Command::new("cmd.exe")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(f.root.join("junction"))
        .arg(&outside)
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    assert!(status.status.success());
    assert!(f
        .call("workspace_read", json!({"path":"junction/safe.txt"}))
        .is_err());
    fs::remove_dir(f.root.join("junction")).unwrap();
    assert!(outside.join("safe.txt").is_file());
    fs::remove_dir_all(outside).unwrap();
}
#[test]
fn partial_read_search_and_budgets() {
    let f = Fixture::new();
    fs::write(f.root.join("large.txt"), "synthetic\n".repeat(100000)).unwrap();
    let v = f
        .call(
            "workspace_read",
            json!({"path":"large.txt","offset":90000,"limit":2}),
        )
        .unwrap();
    assert_eq!(v["lines"].as_array().unwrap().len(), 2);
    assert_eq!(v["sha256"], Value::Null);
    let mut b = CallBudget::new(5);
    for _ in 0..3 {
        b.check("read", "{}").unwrap();
    }
    assert!(b.check("read", "{}").is_err());
    b.check("other", "{}").unwrap();
    assert!(b.exhausted());
    assert!(b.check("last", "{}").is_err());
}
#[test]
fn defaults_policy_schema_and_restart_history() {
    assert!(!Config::default().permissions.enabled);
    let f = Fixture::new();
    let mut cfg = load(&f.c);
    cfg.permissions.allow_push = true;
    assert!(action(&f.c, "save", json!(cfg)).is_err());
    for def in definitions(&f.c) {
        assert_eq!(def["function"]["parameters"]["additionalProperties"], false);
        assert_ne!(def["function"]["name"], "developer_decide");
    }
    begin(&f.c, "restart", "Synthetic interrupted task");
    db(&f.c)
        .unwrap()
        .execute("UPDATE tasks SET owner=4294967295 WHERE id='restart'", [])
        .unwrap();
    let h = action(&f.c, "history", json!({})).unwrap();
    assert_eq!(h["tasks"][0]["state"], "interrupted");
}
#[test]
fn native_approval_is_one_shot_and_cannot_be_forged_as_tool() {
    let id = crate::controle::sufixo_unico();
    let cancel = AtomicBool::new(false);
    assert!(approval(&id, "synthetic", &json!({}), &cancel, &|v| {
        assert_eq!(v["state"], "awaiting_approval");
        decide(&id, true).unwrap();
    })
    .unwrap());
    assert!(decide(&id, true).is_err());
    let f = Fixture::new();
    assert!(f.call("developer_decide", json!({"allow":true})).is_err());
}
#[test]
fn job_runner_output_failure_timeout_and_cancel() {
    let f = Fixture::new();
    let shell = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let run = |source: &str, timeout, cap, cancel: &AtomicBool| {
        crate::developer_runner::run(
            &shell,
            "powershell",
            json!({"source":source}),
            &f.root,
            &private(&f.c).unwrap(),
            timeout,
            cap,
            cancel,
            &|_| {},
        )
        .unwrap()
    };
    let no = AtomicBool::new(false);
    let v = run("Write-Output 'synthetic'; exit 0", 60, 4096, &no);
    assert_eq!(v["state"], "completed");
    assert!(v["stdout"].as_str().unwrap().contains("synthetic"));
    assert_eq!(run("exit 7", 60, 4096, &no)["state"], "failed");
    assert_eq!(
        run("Start-Sleep -Seconds 8", 1, 4096, &no)["state"],
        "timeout"
    );
    assert_eq!(
        run("Start-Sleep -Seconds 8", 10, 4096, &AtomicBool::new(true))["state"],
        "cancelled"
    );
    assert_eq!(
        run(
            &format!("Write-Output '{}'", "x".repeat(6000)),
            60,
            512,
            &no
        )["truncated"],
        true
    );
}

#[test]
fn invalid_execution_scope_and_policy_change_fail_closed() {
    let f = Fixture::new();
    let mut cfg = load(&f.c);
    cfg.permissions.allow_execute = true;
    cfg.permissions.python = "relative.exe".into();
    action(&f.c, "save", json!(cfg)).unwrap();
    assert!(f.call("developer_execute",json!({"executable":"python","arguments":["test.py"],"cwd":"../outside","reason":"Synthetic"})).is_err());
    assert!(f
        .call(
            "developer_execute",
            json!({"executable":"python","arguments":["test.py"],"reason":"Synthetic"})
        )
        .is_err());
    assert!(f.call("developer_execute",json!({"executable":"powershell","arguments":["-Command","Get-Content x"],"reason":"Synthetic"})).is_err());
    assert!(execute_with(
        &f.c,
        "policy-change",
        "workspace_write",
        json!({"path":"a","content":"synthetic","reason":"Synthetic"}),
        &AtomicBool::new(false),
        &|_| {},
        &|_, _| {
            let mut cfg = load(&f.c);
            cfg.permissions.allow_write = false;
            action(&f.c, "save", json!(cfg)).unwrap();
            Ok(true)
        }
    )
    .is_err());
    assert!(!f.root.join("a").exists());
}

#[test]
fn directory_changed_after_proposal_and_invalid_search_are_refused() {
    let f = Fixture::new();
    fs::create_dir(f.root.join("dir")).unwrap();
    assert!(execute_with(
        &f.c,
        "dir-conflict",
        "workspace_move",
        json!({"path":"dir","destination":"other","reason":"Synthetic"}),
        &AtomicBool::new(false),
        &|_| {},
        &|_, _| {
            fs::write(f.root.join("dir/new.txt"), "synthetic").unwrap();
            Ok(true)
        }
    )
    .is_err());
    assert!(f.root.join("dir/new.txt").exists());
    assert!(!f.root.join("other").exists());
    assert!(f
        .call("workspace_search", json!({"query":"x","mode":"execute"}))
        .is_err());
    assert!(f
        .call(
            "workspace_delete",
            json!({"path":"dir","destination":"x","reason":"Synthetic"})
        )
        .is_err());
}

#[test]
fn symlink_and_unapproved_reads_are_blocked() {
    let f = Fixture::new();
    fs::write(f.root.join("source.txt"), "synthetic").unwrap();
    match std::os::windows::fs::symlink_file(f.root.join("source.txt"),f.root.join("symbolic.txt")) {
        Ok(())=>{assert!(f.call("workspace_read",json!({"path":"symbolic.txt"})).is_err());fs::remove_file(f.root.join("symbolic.txt")).unwrap();},
        Err(e) if e.raw_os_error()==Some(1314)=>eprintln!("Synthetic symlink creation unavailable without SeCreateSymbolicLinkPrivilege; junction test remains mandatory."),
        Err(e)=>panic!("Synthetic symlink creation failed: {e}"),
    }
    let mut cfg = load(&f.c);
    cfg.permissions.allow_read = false;
    action(&f.c, "save", json!(cfg)).unwrap();
    assert!(f
        .call("workspace_read", json!({"path":"source.txt"}))
        .is_err());
}
