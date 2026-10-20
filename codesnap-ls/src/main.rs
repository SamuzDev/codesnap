fn main() -> anyhow::Result<()> {
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("codesnap-ls {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--themes" => {
                for theme in codesnap_ls::highlight::available_themes() {
                    println!("{theme}");
                }
                return Ok(());
            }
            other => {
                eprintln!("codesnap-ls: ignoring unknown argument {other:?}");
            }
        }
    }

    codesnap_ls::server::run()
}
