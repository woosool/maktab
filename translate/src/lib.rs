//! Offline translation for Qalam and Sard: OPUS-MT (Marian) models exported to ONNX, run
//! with tract (pure Rust, so every platform builds). A language pair's model is downloaded
//! once (or installed from a folder) into the models folder; after that translating needs no
//! network. Pairs without a model of their own go through English.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tract_onnx::prelude::*;

/// Languages offered: (code, English name).
pub const LANGUAGES: &[(&str, &str)] = &[("en", "English"), ("ar", "Arabic"), ("fr", "French"), ("es", "Spanish"), ("de", "German"), ("it", "Italian"), ("pt", "Portuguese"), ("he", "Hebrew"), ("zh", "Chinese"), ("ja", "Japanese"), ("ru", "Russian"), ("nl", "Dutch"), ("tr", "Turkish")];

/// A model for one direction: its repository and, for multilingual models, the target token.
#[derive(Clone, Debug, PartialEq)]
pub struct Route { pub repo: &'static str, pub prefix: Option<&'static str> }

/// The model for a direct translation from `from` to `to`, if there is one.
pub fn route(from: &str, to: &str) -> Option<Route> {
    let r = |repo: &'static str| Some(Route { repo, prefix: None });
    let p = |repo: &'static str, t: &'static str| Some(Route { repo, prefix: Some(t) });
    match (from, to) {
        ("en", "fr") => r("Xenova/opus-mt-en-fr"), ("en", "es") => r("Xenova/opus-mt-en-es"), ("en", "de") => r("Xenova/opus-mt-en-de"),
        ("en", "it") => r("Xenova/opus-mt-en-it"), ("en", "ar") => p("Xenova/opus-mt-en-ar", ">>ara<<"), ("en", "zh") => p("Xenova/opus-mt-en-zh", ">>cmn_Hans<<"),
        ("en", "ja") => r("Xenova/opus-mt-en-jap"), ("en", "ru") => r("Xenova/opus-mt-en-ru"), ("en", "nl") => r("Xenova/opus-mt-en-nl"),
        ("en", "pt") => p("Xenova/opus-mt-en-ROMANCE", ">>pt<<"), ("en", "he") => p("Xenova/opus-mt-en-mul", ">>heb<<"), ("en", "tr") => p("Xenova/opus-mt-en-mul", ">>tur<<"),
        ("fr", "en") => r("Xenova/opus-mt-fr-en"), ("es", "en") => r("Xenova/opus-mt-es-en"), ("de", "en") => r("Xenova/opus-mt-de-en"),
        ("it", "en") => r("Xenova/opus-mt-it-en"), ("ar", "en") => r("Xenova/opus-mt-ar-en"), ("zh", "en") => r("Xenova/opus-mt-zh-en"),
        ("ja", "en") => r("Xenova/opus-mt-ja-en"), ("ru", "en") => r("Xenova/opus-mt-ru-en"), ("nl", "en") => r("Xenova/opus-mt-nl-en"),
        ("tr", "en") => r("Xenova/opus-mt-tr-en"), ("pt", "en") => r("Xenova/opus-mt-ROMANCE-en"), ("he", "en") => r("Xenova/opus-mt-mul-en"),
        ("fr", "es") => r("Xenova/opus-mt-fr-es"), ("es", "fr") => r("Xenova/opus-mt-es-fr"), ("fr", "de") => r("Xenova/opus-mt-fr-de"), ("de", "fr") => r("Xenova/opus-mt-de-fr"),
        _ => None,
    }
}

/// The models a translation needs, in order (one, or two through English).
pub fn plan(from: &str, to: &str) -> Result<Vec<Route>, String> {
    if from == to { return Ok(vec![]); }
    if let Some(r) = route(from, to) { return Ok(vec![r]); }
    match (route(from, "en"), route("en", to)) {
        (Some(a), Some(b)) => Ok(vec![a, b]),
        _ => Err(format!("No translation model for {from} → {to}.")),
    }
}

const FILES: &[(&str, &str)] = &[("config.json", "config.json"), ("tokenizer.json", "tokenizer.json"), ("onnx/encoder_model_quantized.onnx", "encoder.onnx"), ("onnx/decoder_model_quantized.onnx", "decoder.onnx")];

fn dir_of(models: &Path, repo: &str) -> PathBuf { models.join(repo.rsplit('/').next().unwrap_or(repo)) }

/// Is this model in the models folder?
pub fn installed(models: &Path, repo: &str) -> bool { FILES.iter().all(|(_, f)| dir_of(models, repo).join(f).is_file()) }

/// The models missing for a translation.
pub fn missing(models: &Path, from: &str, to: &str) -> Result<Vec<&'static str>, String> {
    let mut v: Vec<&'static str> = plan(from, to)?.into_iter().map(|r| r.repo).filter(|r| !installed(models, r)).collect();
    v.dedup();
    Ok(v)
}

/// Installed models: (repo folder name, size in bytes).
pub fn list_installed(models: &Path) -> Vec<(String, u64)> {
    let Ok(rd) = std::fs::read_dir(models) else { return vec![] };
    let mut v: Vec<(String, u64)> = rd.flatten().filter(|e| e.path().join("decoder.onnx").is_file()).map(|e| {
        let size = std::fs::read_dir(e.path()).map(|r| r.flatten().filter_map(|f| f.metadata().ok()).map(|m| m.len()).sum()).unwrap_or(0);
        (e.file_name().to_string_lossy().into_owned(), size)
    }).collect();
    v.sort();
    v
}

pub fn remove(models: &Path, folder: &str) -> std::io::Result<()> {
    forget(folder);
    std::fs::remove_dir_all(models.join(folder))
}

/// Download progress: bytes done and expected (all files of the current download).
pub static DONE: AtomicU64 = AtomicU64::new(0);
pub static TOTAL: AtomicU64 = AtomicU64::new(0);

/// Download a model (once; it's then used offline). Files land complete or not at all.
pub fn download(models: &Path, repo: &str) -> Result<(), String> {
    let dir = dir_of(models, repo);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    DONE.store(0, Ordering::Relaxed);
    TOTAL.store(0, Ordering::Relaxed);
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(1800))).build().into();
    for (src, name) in FILES {
        let dest = dir.join(name);
        if dest.is_file() { continue; }
        let url = format!("https://huggingface.co/{repo}/resolve/main/{src}");
        let mut resp = agent.get(&url).call().map_err(|e| format!("Couldn't download {name}: {e}"))?;
        if let Some(n) = resp.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok()) { TOTAL.fetch_add(n, Ordering::Relaxed); }
        let part = dir.join(format!("{name}.part"));
        let mut f = std::fs::File::create(&part).map_err(|e| e.to_string())?;
        let mut r = resp.body_mut().as_reader();
        let mut buf = vec![0u8; 1 << 16];
        loop {
            use std::io::{Read, Write};
            let n = r.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 { break; }
            f.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            DONE.fetch_add(n as u64, Ordering::Relaxed);
        }
        drop(f);
        std::fs::rename(&part, &dest).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Install a model from a folder holding its files (for computers that are never online):
/// config.json, tokenizer.json and the encoder and decoder (any of the usual file names).
pub fn install_from(models: &Path, repo: &str, from: &Path) -> Result<(), String> {
    let dir = dir_of(models, repo);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let find = |names: &[&str]| -> Option<PathBuf> { names.iter().flat_map(|n| [from.join(n), from.join("onnx").join(n)]).find(|p| p.is_file()) };
    let pick = [
        ("config.json", find(&["config.json"])),
        ("tokenizer.json", find(&["tokenizer.json"])),
        ("encoder.onnx", find(&["encoder.onnx", "encoder_model_quantized.onnx", "encoder_model.onnx"])),
        ("decoder.onnx", find(&["decoder.onnx", "decoder_model_quantized.onnx", "decoder_model.onnx"])),
    ];
    for (name, src) in pick {
        let src = src.ok_or_else(|| format!("{name} wasn't found in that folder."))?;
        std::fs::copy(&src, dir.join(name)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---- running a model ----------------------------------------------------------------------------

type Plan = Arc<TypedRunnableModel>;

pub struct Model { enc: Plan, dec: Plan, tok: tokenizers::Tokenizer, start: i64, eos: i64, pad: i64, d_model: usize }

fn load_onnx(path: &Path) -> Result<InferenceModel, String> {
    let onnx = tract_onnx::onnx();
    let mut proto = onnx.proto_model_for_path(path).map_err(|e| e.to_string())?;
    // The exported shapes of inner values use names tract can't unify; it works them out itself.
    if let Some(g) = proto.graph.as_mut() { g.value_info.clear(); for o in g.output.iter_mut() { o.r#type = None; } }
    onnx.model_for_proto_model(&proto).map_err(|e| e.to_string())
}

impl Model {
    pub fn load(dir: &Path) -> Result<Model, String> {
        let cfg: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("config.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let num = |k: &str, d: i64| cfg[k].as_i64().unwrap_or(d);
        let d_model = num("d_model", 512) as usize;
        let mut enc = load_onnx(&dir.join("encoder.onnx"))?;
        let s = enc.symbols.sym("S");
        for i in 0..2 { enc.set_input_fact(i, InferenceFact::dt_shape(i64::datum_type(), tvec![1.to_dim(), s.to_dim()])).map_err(|e| e.to_string())?; }
        let enc = enc.into_optimized().map_err(|e| format!("{e:?}"))?.into_runnable().map_err(|e| e.to_string())?;
        let mut dec = load_onnx(&dir.join("decoder.onnx"))?;
        let (s, t) = (dec.symbols.sym("S"), dec.symbols.sym("T"));
        // Inputs by name: the order differs between exports.
        let names: Vec<String> = dec.input_outlets().map_err(|e| e.to_string())?.iter().map(|o| dec.node(o.node).name.clone()).collect();
        for (i, n) in names.iter().enumerate() {
            let fact = match n.as_str() {
                "encoder_attention_mask" => InferenceFact::dt_shape(i64::datum_type(), tvec![1.to_dim(), s.to_dim()]),
                "input_ids" => InferenceFact::dt_shape(i64::datum_type(), tvec![1.to_dim(), t.to_dim()]),
                "encoder_hidden_states" => InferenceFact::dt_shape(f32::datum_type(), tvec![1.to_dim(), s.to_dim(), d_model.to_dim()]),
                other => return Err(format!("Unexpected decoder input {other} (a model with past key values isn't supported).")),
            };
            dec.set_input_fact(i, fact).map_err(|e| e.to_string())?;
        }
        let order: Vec<String> = names;
        let dec = dec.into_optimized().map_err(|e| format!("{e:?}"))?.into_runnable().map_err(|e| e.to_string())?;
        // Some exports leave an empty "Precompiled" normalizer, which reads as an error.
        let mut tj: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("tokenizer.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        if tj["normalizer"]["type"] == "Precompiled" && tj["normalizer"]["precompiled_charsmap"].is_null() { tj["normalizer"] = serde_json::Value::Null; }
        let tok: tokenizers::Tokenizer = std::str::FromStr::from_str(&tj.to_string()).map_err(|e: tokenizers::Error| e.to_string())?;
        let pad = num("pad_token_id", 0);
        let m = Model { enc, dec, tok, start: num("decoder_start_token_id", pad), eos: num("eos_token_id", 0), pad, d_model };
        DEC_ORDER.lock().unwrap().insert(dir.to_path_buf(), order);
        Ok(m)
    }

    /// Translate one sentence (greedy decoding). `prefix`: a target-language token for
    /// multilingual models.
    pub fn sentence(&self, text: &str, prefix: Option<&str>, dir: &Path) -> Result<String, String> {
        let enc = self.tok.encode(text, true).map_err(|e| e.to_string())?;
        let mut ids: Vec<i64> = enc.get_ids().iter().map(|&i| i as i64).collect();
        // A multilingual model's target-language token goes first, as one token.
        if let Some(id) = prefix.and_then(|p| self.tok.token_to_id(p)) { ids.insert(0, id as i64); }
        if ids.last() != Some(&self.eos) { ids.push(self.eos); }
        ids.truncate(400);
        let n = ids.len();
        let ids_t: Tensor = tract_ndarray::Array2::from_shape_vec((1, n), ids).map_err(|e| e.to_string())?.into_tensor();
        let mask: Tensor = tract_ndarray::Array2::from_elem((1, n), 1i64).into_tensor();
        let hidden = self.enc.run(tvec![ids_t.into(), mask.clone().into()]).map_err(|e| e.to_string())?.remove(0);
        let order = DEC_ORDER.lock().unwrap().get(dir).cloned().unwrap_or_default();
        let mut out: Vec<i64> = vec![self.start];
        let max = (n * 2 + 16).min(512);
        while out.len() < max {
            let t = out.len();
            let dec_ids: Tensor = tract_ndarray::Array2::from_shape_vec((1, t), out.clone()).map_err(|e| e.to_string())?.into_tensor();
            let inputs: TVec<TValue> = order.iter().map(|name| match name.as_str() {
                "encoder_attention_mask" => mask.clone().into(),
                "input_ids" => dec_ids.clone().into(),
                _ => hidden.clone(),
            }).collect();
            let logits = self.dec.run(inputs).map_err(|e| e.to_string())?.remove(0);
            let view = logits.to_plain_array_view::<f32>().map_err(|e| e.to_string())?;
            let vocab = view.shape()[2];
            // The pad token is never produced.
            let (best, _) = (0..vocab).filter(|&v| v as i64 != self.pad).map(|v| (v, view[[0, t - 1, v]])).fold((0, f32::MIN), |a, b| if b.1 > a.1 { b } else { a });
            if best as i64 == self.eos { break; }
            out.push(best as i64);
        }
        let toks: Vec<u32> = out.iter().skip(1).map(|&i| i as u32).collect();
        let _ = self.d_model;
        self.tok.decode(&toks, true).map_err(|e| e.to_string())
    }
}

static DEC_ORDER: Mutex<std::collections::BTreeMap<PathBuf, Vec<String>>> = Mutex::new(std::collections::BTreeMap::new());

/// Loaded models (two at most: each takes about 100–300 MB).
static LOADED: Mutex<Vec<(PathBuf, Arc<Model>)>> = Mutex::new(Vec::new());

fn model(dir: &Path) -> Result<Arc<Model>, String> {
    {
        let mut l = LOADED.lock().unwrap();
        if let Some(i) = l.iter().position(|m| m.0 == dir) { let e = l.remove(i); let m = e.1.clone(); l.push(e); return Ok(m); }
    }
    let m = Arc::new(Model::load(dir)?);
    let mut l = LOADED.lock().unwrap();
    l.push((dir.to_path_buf(), m.clone()));
    while l.len() > 2 { l.remove(0); }
    Ok(m)
}

/// Unload a model (frees its memory).
pub fn forget(folder: &str) { LOADED.lock().unwrap().retain(|m| !m.0.ends_with(folder)); }

/// Sentences of a paragraph (kept with their ending punctuation).
pub fn sentences(p: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let chars: Vec<char> = p.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        cur.push(c);
        let end = matches!(c, '.' | '!' | '?' | '؟' | '。' | '！' | '？') && chars.get(i + 1).is_none_or(|n| n.is_whitespace());
        if end && cur.trim().chars().count() > 1 { out.push(std::mem::take(&mut cur).trim().to_string()); }
    }
    if !cur.trim().is_empty() { out.push(cur.trim().to_string()); }
    out
}

/// Translate text (paragraphs kept) from `from` to `to` with the installed models;
/// `progress` gets (sentences done, total).
pub fn translate(models: &Path, from: &str, to: &str, text: &str, progress: &dyn Fn(usize, usize)) -> Result<String, String> {
    let steps = plan(from, to)?;
    if steps.is_empty() { return Ok(text.to_string()); }
    if let Some(m) = steps.iter().find(|r| !installed(models, r.repo)) { return Err(format!("The model {} isn't installed.", m.repo)); }
    let paras: Vec<&str> = text.split('\n').collect();
    let total: usize = paras.iter().map(|p| sentences(p).len()).sum::<usize>() * steps.len();
    let mut done = 0;
    let mut cur: Vec<String> = paras.iter().map(|s| s.to_string()).collect();
    for step in &steps {
        let dir = dir_of(models, step.repo);
        let m = model(&dir)?;
        cur = cur.iter().map(|p| {
            if p.trim().is_empty() { return Ok(p.clone()); }
            let parts: Result<Vec<String>, String> = sentences(p).iter().map(|s| { let r = m.sentence(s, step.prefix, &dir); done += 1; progress(done, total.max(1)); r }).collect();
            Ok(parts?.join(if matches!(to, "zh" | "ja") { "" } else { " " }))
        }).collect::<Result<Vec<_>, String>>()?;
    }
    Ok(cur.join("\n"))
}

/// A guess at the language of a text (by script, then common words).
pub fn detect(text: &str) -> &'static str {
    let count = |f: &dyn Fn(char) -> bool| text.chars().filter(|&c| f(c)).count();
    let letters = count(&|c| c.is_alphabetic()).max(1);
    let share = |n: usize| n * 3 > letters;
    if share(count(&|c| ('\u{0600}'..='\u{06FF}').contains(&c))) { return "ar"; }
    if share(count(&|c| ('\u{0590}'..='\u{05FF}').contains(&c))) { return "he"; }
    if share(count(&|c| ('\u{3040}'..='\u{30FF}').contains(&c))) { return "ja"; }
    if share(count(&|c| ('\u{4E00}'..='\u{9FFF}').contains(&c))) { return "zh"; }
    if share(count(&|c| ('\u{0400}'..='\u{04FF}').contains(&c))) { return "ru"; }
    let words: Vec<String> = text.to_lowercase().split(|c: char| !c.is_alphabetic()).filter(|w| !w.is_empty()).map(String::from).collect();
    let score = |list: &[&str]| words.iter().filter(|w| list.contains(&w.as_str())).count();
    let langs: [(&'static str, &[&str]); 8] = [
        ("en", &["the", "and", "is", "of", "to", "in", "that", "it", "with", "for", "this", "are"]),
        ("fr", &["le", "la", "les", "et", "est", "des", "une", "un", "dans", "pour", "que", "avec", "du"]),
        ("es", &["el", "los", "las", "es", "y", "del", "una", "por", "con", "para", "que", "se"]),
        ("de", &["der", "die", "das", "und", "ist", "nicht", "mit", "ein", "eine", "zu", "auf", "den"]),
        ("it", &["il", "gli", "è", "di", "che", "per", "una", "con", "non", "della", "sono"]),
        ("pt", &["o", "os", "as", "é", "não", "uma", "com", "para", "que", "do", "da", "em"]),
        ("nl", &["de", "het", "een", "en", "is", "van", "niet", "met", "voor", "dat", "zijn"]),
        ("tr", &["ve", "bir", "bu", "için", "ile", "da", "de", "çok", "olarak", "daha"]),
    ];
    langs.iter().max_by_key(|(_, l)| score(l)).filter(|(_, l)| score(l) > 0).map(|(c, _)| *c).unwrap_or("en")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_and_detection() {
        assert_eq!(plan("ar", "fr").unwrap().iter().map(|r| r.repo).collect::<Vec<_>>(), ["Xenova/opus-mt-ar-en", "Xenova/opus-mt-en-fr"]);
        assert_eq!(plan("en", "pt").unwrap()[0].prefix, Some(">>pt<<"));
        for (a, _) in LANGUAGES { for (b, _) in LANGUAGES { if a != b { assert!(plan(a, b).is_ok(), "{a}->{b}"); } } }
        assert_eq!(detect("مرحبا بكم في العرض"), "ar");
        assert_eq!(detect("Le chat est dans la maison"), "fr");
        assert_eq!(detect("The cat is in the house"), "en");
        assert_eq!(sentences("Hello there. How are you? Fine!"), vec!["Hello there.", "How are you?", "Fine!"]);
    }

    /// With a model folder (TRANSLATE_MODELS) holding opus-mt-en-fr, a real translation.
    #[test]
    fn translates_offline() {
        let Ok(dir) = std::env::var("TRANSLATE_MODELS") else { return };
        let models = PathBuf::from(dir);
        if !installed(&models, "Xenova/opus-mt-en-fr") { return; }
        let t = std::time::Instant::now();
        let out = translate(&models, "en", "fr", "Hello, how are you today?\nThe presentation starts at noon.", &|_, _| {}).unwrap();
        eprintln!("{out:?} in {:?}", t.elapsed());
        assert!(out.to_lowercase().contains("bonjour") || out.to_lowercase().contains("comment"), "{out}");
        assert_eq!(out.lines().count(), 2);
    }
}
