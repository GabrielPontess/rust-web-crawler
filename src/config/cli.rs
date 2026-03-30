use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "crawler", about = "Crawler MVP CLI", version)]
pub struct CliArgs {
    /// Caminho para o arquivo JSON de configuração (ex.: appsettings.json)
    #[arg(
        short = 'c',
        long = "config",
        env = "CRAWLER_CONFIG",
        default_value = "appsettings.json"
    )]
    pub config_path: String,
}
