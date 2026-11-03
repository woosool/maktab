fn main() {
    let models = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let (from, to, text) = (std::env::args().nth(2).unwrap(), std::env::args().nth(3).unwrap(), std::env::args().nth(4).unwrap());
    for m in woffice_translate::missing(&models, &from, &to).unwrap() { eprintln!("downloading {m}"); woffice_translate::download(&models, m).unwrap(); }
    let t = std::time::Instant::now();
    println!("{}", woffice_translate::translate(&models, &from, &to, &text, &|_, _| {}).unwrap());
    eprintln!("{:?}", t.elapsed());
}
