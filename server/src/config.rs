pub struct Config { pub bind: String, pub database_url: String, pub jwt_secret: String }
impl Config {
    pub fn from_env() -> Self {
        Self {
            bind: std::env::var("BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_string()),
            database_url: std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://ruggine.sqlite".to_string()),
            jwt_secret: std::env::var("JWT_SECRET").unwrap_or_else(|_| "dev_secret_change_me".to_string()),
        }
    }
}