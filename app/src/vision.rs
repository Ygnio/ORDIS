//! Private, bounded attachments. Re-encoding strips EXIF and embedded metadata.
use crate::controle::Caminhos;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use image::{ImageDecoder, ImageFormat, ImageReader};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub enabled: bool,
    pub max_mb: usize,
    pub max_edge: u32,
    pub retention_days: u32,
    pub history_images: usize,
    pub screenshot: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            max_mb: 12,
            max_edge: 2048,
            retention_days: 0,
            history_images: 4,
            screenshot: false,
        }
    }
}
fn private_root(c: &Caminhos) -> Result<PathBuf, String> {
    let db = crate::memory::database_path(c)?;
    let parent = db.parent().ok_or("Pasta privada inválida.")?;
    let root = parent.join("vision");
    if root.exists()
        && !root
            .canonicalize()
            .map_err(|_| "Pasta privada inválida.")?
            .starts_with(
                parent
                    .canonicalize()
                    .map_err(|_| "Pasta privada inválida.")?,
            )
    {
        return Err("A pasta de anexos aponta para fora do perfil privado.".into());
    }
    Ok(root)
}
pub fn config(c: &Caminhos) -> Config {
    private_root(c)
        .ok()
        .and_then(|p| fs::read(p.join("config.json")).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .filter(|c: &Config| valid(c))
        .unwrap_or_default()
}
fn valid(c: &Config) -> bool {
    (1..=20).contains(&c.max_mb)
        && (256..=4096).contains(&c.max_edge)
        && c.retention_days <= 30
        && c.history_images <= 8
}
fn cancelled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("Processamento de imagem cancelado.".into())
    } else {
        Ok(())
    }
}
pub fn process(bytes: &[u8], cfg: &Config, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
    cancelled(cancel)?;
    if !valid(cfg) || bytes.len() > cfg.max_mb * 1024 * 1024 {
        return Err("Imagem acima do limite configurado.".into());
    }
    let format = image::guess_format(bytes).map_err(|_| "Formato real inválido.")?;
    if !matches!(
        format,
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP
    ) {
        return Err("Use PNG, JPEG ou WebP.".into());
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader
        .into_decoder()
        .map_err(|_| "Imagem malformada ou dimensões excessivas.")?;
    let (w, h) = decoder.dimensions();
    if w == 0
        || h == 0
        || (w as u64) * (h as u64) > 32_000_000
        || decoder.total_bytes() > 128 * 1024 * 1024
    {
        return Err("Dimensões excessivas.".into());
    }
    let orientation = decoder.orientation().map_err(|_| "Orientação inválida.")?;
    cancelled(cancel)?;
    let mut image = image::DynamicImage::from_decoder(decoder).map_err(|_| "Imagem malformada.")?;
    image.apply_orientation(orientation);
    cancelled(cancel)?;
    if image.width() > cfg.max_edge || image.height() > cfg.max_edge {
        image = image.resize(
            cfg.max_edge,
            cfg.max_edge,
            image::imageops::FilterType::Triangle,
        );
    }
    let mut output = Cursor::new(Vec::new());
    image
        .write_to(&mut output, ImageFormat::Png)
        .map_err(|_| "Falha ao normalizar imagem.")?;
    cancelled(cancel)?;
    let output = output.into_inner();
    if output.len() > 32 * 1024 * 1024 {
        return Err("Imagem normalizada excessiva.".into());
    }
    Ok(output)
}
pub fn save_batch(
    c: &Caminhos,
    urls: &[String],
    cancel: &AtomicBool,
) -> Result<Vec<String>, String> {
    if urls.is_empty() {
        return Ok(vec![]);
    }
    let cfg = config(c);
    if !cfg.enabled {
        return Err("Entrada de imagens desabilitada nas configurações de Visão.".into());
    }
    if urls.len() > 4 {
        return Err("Até quatro imagens por mensagem.".into());
    }
    let (endpoint, _) = crate::backends::active_endpoint(c);
    let u = url::Url::parse(&endpoint).map_err(|_| "Backend inválido.")?;
    if !matches!(u.host_str(), Some("127.0.0.1" | "[::1]" | "localhost")) {
        return Err("Imagens só podem ser enviadas a backend local.".into());
    }
    if !crate::backends::active_capabilities(c).image_input {
        return Err("O backend não anuncia suporte a imagens.".into());
    }
    let root = private_root(c)?;
    fs::create_dir_all(&root).map_err(|_| "Pasta privada indisponível.")?;
    let mut paths = vec![];
    let result = (|| {
        for url in urls {
            cancelled(cancel)?;
            if url.len() > cfg.max_mb * 1024 * 1024 * 4 / 3 + 128 {
                return Err("Anexo excessivo.".into());
            }
            let (header, payload) = url.split_once(',').ok_or("Anexo inválido.")?;
            if ![
                "data:image/png;base64",
                "data:image/jpeg;base64",
                "data:image/webp;base64",
            ]
            .contains(&header)
            {
                return Err("Formato não aceito.".into());
            }
            let bytes = B64.decode(payload).map_err(|_| "Base64 inválido.")?;
            let normalized = process(&bytes, &cfg, cancel)?;
            let path = root.join(format!("{}.png", crate::controle::sufixo_unico()));
            let expires = if cfg.retention_days == 0 {
                0
            } else {
                chrono::Utc::now().timestamp() + cfg.retention_days as i64 * 86400
            };
            fs::write(&path, normalized).map_err(|_| "Falha ao salvar anexo privado.")?;
            paths.push(path.display().to_string());
            fs::write(
                path.with_extension("json"),
                json!({"expires":expires}).to_string(),
            )
            .map_err(|_| "Falha ao salvar retenção.")?;
        }
        cancelled(cancel)?;
        Ok(paths.clone())
    })();
    if result.is_err() {
        remove(&paths);
    }
    result
}
pub fn remove(paths: &[String]) {
    for path in paths {
        let p = Path::new(path);
        let _ = fs::remove_file(p);
        let _ = fs::remove_file(p.with_extension("json"));
    }
}
/// Startup deletes ephemeral screenshots/attachments; expired references remain textual in history.
pub fn cleanup(c: &Caminhos, startup: bool) {
    if let Ok(root) = private_root(c) {
        if let Ok(files) = fs::read_dir(root) {
            for e in files.flatten() {
                let p = e.path();
                if p.extension().and_then(|s| s.to_str()) != Some("png") {
                    continue;
                }
                let expires = fs::read(p.with_extension("json"))
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                    .and_then(|v| v["expires"].as_i64());
                if expires.is_none_or(|t| {
                    t < 0 || t > 0 && t <= chrono::Utc::now().timestamp() || t == 0 && startup
                }) {
                    remove(&[p.display().to_string()]);
                }
            }
        }
    }
}
pub fn data_url(c: &Caminhos, path: &Path) -> Result<String, String> {
    let root = private_root(c)?;
    let actual = path
        .canonicalize()
        .map_err(|_| "Anexo expirado ou removido.")?;
    if !actual.starts_with(
        root.canonicalize()
            .map_err(|_| "Pasta privada indisponível.")?,
    ) {
        return Err("Anexo fora da pasta privada.".into());
    }
    let expires = fs::read(path.with_extension("json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v["expires"].as_i64())
        .ok_or("Retenção inválida.")?;
    if expires < 0 || expires > 0 && expires <= chrono::Utc::now().timestamp() {
        return Err("Anexo expirado.".into());
    }
    let meta = fs::metadata(&actual).map_err(|_| "Anexo indisponível.")?;
    if meta.len() > 32 * 1024 * 1024 {
        return Err("Anexo excessivo.".into());
    }
    Ok(format!(
        "data:image/png;base64,{}",
        B64.encode(fs::read(actual).map_err(|_| "Anexo indisponível.")?)
    ))
}
fn probe_confirmed(state: &str, text: &str) -> bool {
    if state != "completed" {
        return false;
    }
    let text = text.to_lowercase();
    ["red", "blue", "square", "circle"]
        .iter()
        .all(|word| text.contains(word))
}

pub fn action(c: &Caminhos, name: &str, args: Value) -> Result<Value, String> {
    match name {
        "cleanup_ephemeral" => {
            cleanup(c, true);
            Ok(json!(true))
        }
        "probe" => {
            let cfg = config(c);
            let mut picture = image::RgbImage::from_pixel(320, 160, image::Rgb([255, 255, 255]));
            for y in 30..130 {
                for x in 20..120 {
                    picture.put_pixel(x, y, image::Rgb([255, 0, 0]));
                }
            }
            for y in 30..130 {
                for x in 190..290 {
                    if (x as i32 - 240).pow(2) + (y as i32 - 80).pow(2) < 2500 {
                        picture.put_pixel(x, y, image::Rgb([0, 0, 255]));
                    }
                }
            }
            let mut png = Cursor::new(vec![]);
            picture
                .write_to(&mut png, ImageFormat::Png)
                .map_err(|_| "Imagem de teste indisponível.")?;
            let (endpoint, _) = crate::backends::active_endpoint(c);
            let u = url::Url::parse(&endpoint).map_err(|_| "Endpoint inválido.")?;
            if !matches!(u.host_str(), Some("127.0.0.1" | "[::1]")) {
                return Err("Teste visual exige HTTP loopback literal.".into());
            }
            if !crate::backends::active_capabilities(c).image_input {
                return Err("Backend não anuncia visão.".into());
            }
            let start = std::time::Instant::now();
            let messages = vec![
                json!({"role":"user","content":[{"type":"text","text":"Describe the colors and shapes in this image in English. Be brief."},{"type":"image_url","image_url":{"url":format!("data:image/png;base64,{}",B64.encode(png.into_inner()))}}]}),
            ];
            let result = crate::backends::vision_probe(c, &messages)?;
            let confirmed = probe_confirmed(&result.metrics.state, &result.text);
            let report = json!({"confirmed":confirmed,"elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"metrics":result.metrics,"model":crate::backends::selected_model(c)?,"endpoint":endpoint,"retention_days":cfg.retention_days,"test":"synthetic_shapes_v1","vram_impact":null});
            let root = private_root(c)?;
            fs::create_dir_all(&root).map_err(|_| "Pasta privada indisponível.")?;
            fs::write(root.join("probe.json"), report.to_string())
                .map_err(|_| "Falha ao salvar diagnóstico privado.")?;
            Ok(report)
        }
        "capture" => {
            if !config(c).enabled || !config(c).screenshot {
                return Err("Habilite captura de tela nas configurações de Visão.".into());
            }
            // Windows' own picker requires a deliberate selection. Clipboard is only read by
            // the UI's explicit paste gesture, then displayed as a removable draft thumbnail.
            use std::os::windows::process::CommandExt;
            std::process::Command::new("explorer.exe").arg("ms-screenclip:").creation_flags(0x08000000).spawn().map_err(|_|"Não foi possível abrir a Ferramenta de Captura. Use Win+Shift+S e cole a seleção no chat.")?;
            Ok(
                json!({"paste_required":true,"message":"Selecione a região na Ferramenta de Captura e cole no chat (Ctrl+V). Revise a miniatura antes de enviar; Esc cancela a seleção."}),
            )
        }
        "load" => Ok(json!(config(c))),
        "save" => {
            let cfg: Config = serde_json::from_value(args).map_err(|_| "Configuração inválida.")?;
            if !valid(&cfg) {
                return Err("Limites de visão inválidos.".into());
            }
            let root = private_root(c)?;
            fs::create_dir_all(&root).map_err(|_| "Pasta privada indisponível.")?;
            let path = root.join("config.json");
            let tmp = path.with_extension("tmp");
            fs::write(&tmp, json!(cfg).to_string()).map_err(|_| "Falha ao salvar visão.")?;
            fs::rename(tmp, path).map_err(|_| "Falha ao salvar visão.")?;
            cleanup(c, false);
            Ok(json!(true))
        }
        "diagnostic" => {
            let s = crate::backends::check(c)?;
            let previous = private_root(c)
                .ok()
                .and_then(|p| fs::read(p.join("probe.json")).ok())
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .filter(|v| {
                    v["model"].as_str() == s.model.as_deref() && v["endpoint"] == s.endpoint
                });
            let confirmed = previous.as_ref().is_some_and(|v| v["confirmed"] == true);
            Ok(
                json!({"backend":s.backend,"model":s.model,"image_support":if confirmed{"confirmed_by_synthetic_probe"}else if s.capabilities.image_input{"configured_or_advertised_not_yet_verified"}else if s.capabilities.image_input_advertised==Some(false){"unavailable"}else{"unknown"},"previous_probe":previous,"image_configured":s.capabilities.image_input_configured,"image_advertised":s.capabilities.image_input_advertised,"projector":"not_verified","formats":["PNG","JPEG","WebP"],"metadata":"stripped_on_import","context":s.context_tokens,"vram_impact":null,"visual_processing_ms":null,"note":"Uma troca de pesos usando o mesmo alias exige novo teste. Uso de contexto vem do backend, sem custo fixo por imagem."}),
            )
        }
        _ => Err("Ação de visão inválida.".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incomplete_visual_probe_never_confirms_capability() {
        let text = "A red square and a blue circle.";
        assert!(probe_confirmed("completed", text));
        for state in [
            "length",
            "cancelled",
            "timeout",
            "transport_error",
            "protocol_error",
            "no_final",
        ] {
            assert!(!probe_confirmed(state, text));
        }
        assert!(!probe_confirmed("completed", "A red square."));
    }
    fn picture(format: ImageFormat) -> Vec<u8> {
        let mut out = Cursor::new(vec![]);
        image::DynamicImage::new_rgb8(20, 10)
            .write_to(&mut out, format)
            .unwrap();
        out.into_inner()
    }
    #[test]
    fn png_jpeg_and_metadata() {
        for f in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::WebP] {
            let b = process(&picture(f), &Config::default(), &AtomicBool::new(false)).unwrap();
            assert_eq!(image::guess_format(&b).unwrap(), ImageFormat::Png);
        }
    }
    #[test]
    fn malformed_cancelled_oversized() {
        assert!(process(b"not an image", &Config::default(), &AtomicBool::new(false)).is_err());
        assert!(process(
            &picture(ImageFormat::Png),
            &Config::default(),
            &AtomicBool::new(true)
        )
        .is_err());
        assert!(process(
            &vec![0; 13 * 1024 * 1024],
            &Config::default(),
            &AtomicBool::new(false)
        )
        .is_err());
    }
    #[test]
    fn resize_and_config() {
        let mut out = Cursor::new(vec![]);
        image::DynamicImage::new_rgb8(300, 10)
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        let cfg = Config {
            max_edge: 256,
            ..Config::default()
        };
        let b = process(&out.into_inner(), &cfg, &AtomicBool::new(false)).unwrap();
        assert_eq!(image::load_from_memory(&b).unwrap().width(), 256);
        assert!(!valid(&Config { max_mb: 500, ..cfg }));
    }
    #[test]
    fn retention_cleanup_and_private_paths() {
        let root = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap())
            .join("Lucille")
            .join(format!(
                "vision-synthetic-{}",
                crate::controle::sufixo_unico()
            ));
        let c = Caminhos {
            projeto: root.clone(),
            runtime: root.join("runtime"),
        };
        let folder = private_root(&c).unwrap();
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("synthetic.png");
        fs::write(&path, picture(ImageFormat::Png)).unwrap();
        fs::write(path.with_extension("json"), "{\"expires\":0}").unwrap();
        assert!(data_url(&c, &path)
            .unwrap()
            .starts_with("data:image/png;base64,"));
        cleanup(&c, false);
        assert!(path.exists());
        cleanup(&c, true);
        assert!(!path.exists());
        fs::write(&path, picture(ImageFormat::Png)).unwrap();
        fs::write(path.with_extension("json"), "{\"expires\":1}").unwrap();
        assert!(data_url(&c, &path).is_err());
        cleanup(&c, false);
        assert!(!path.exists());
        assert!(data_url(&c, Path::new("Cargo.toml")).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn excessive_dimensions() {
        let mut out = Cursor::new(vec![]);
        image::DynamicImage::new_rgb8(20000, 1)
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        assert!(process(
            &out.into_inner(),
            &Config::default(),
            &AtomicBool::new(false)
        )
        .is_err());
    }
    #[test]
    fn jpeg_orientation_is_applied_and_exif_removed() {
        let original = picture(ImageFormat::Jpeg);
        let exif = b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";
        let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe1];
        jpeg.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
        jpeg.extend_from_slice(exif);
        jpeg.extend_from_slice(&original[2..]);
        let out = process(&jpeg, &Config::default(), &AtomicBool::new(false)).unwrap();
        let im = image::load_from_memory(&out).unwrap();
        assert_eq!((im.width(), im.height()), (10, 20));
        assert!(!out.windows(4).any(|w| w == b"Exif"));
    }
}
