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

    /// Consulta a ser executada no índice em vez de rastrear
    #[arg(long = "search", env = "CRAWLER_SEARCH_QUERY")]
    pub search: Option<String>,

    /// Número máximo de resultados retornados no modo pesquisa
    #[arg(long = "search-limit", default_value_t = 10, requires = "search")]
    pub search_limit: u32,
}
