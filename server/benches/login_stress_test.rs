use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use server::state::AppState;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};
use uuid::Uuid;
use tokio::runtime::Runtime;

/// Setup database con utenti pre-esistenti
async fn create_test_setup(num_users: usize) -> (AppState, Vec<(Uuid, String)>) {
    let pool = SqlitePool::connect(":memory:")
        .await
        .expect("Failed to create test database");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    let state = AppState::new(pool.clone(), "test_secret".to_string());

    let mut users = Vec::new();

    // Crea utenti pre-esistenti nel database
    for i in 0..num_users {
        let username = format!("user_{}", i);
        let user_id = Uuid::new_v4();

        sqlx::query(
            "INSERT INTO users (id, username, pass_hash, created_at)
             VALUES (?, ?, ?, ?)"
        )
            .bind(user_id.to_string())
            .bind(&username)
            .bind("$argon2id$v=19$m=65536,t=3,p=4$eksJ0Nj6JjARShFv6MMsbw$MRYhXaz3fV4es39v3M8IcpO4d1fZXy92KM76Ce8FY/I")
            .bind(chrono::Utc::now().timestamp())
            .execute(&pool)
            .await
            .expect("Failed to create user");

        users.push((user_id, username));
    }

    (state, users)
}

/// Simula una singola connessione WebSocket (senza il WebSocket reale)
async fn simulate_connection(
    state: Arc<AppState>,
    user_id: Uuid,
    username: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Simula il flusso di register_connection
    let session_id = Uuid::new_v4();
    let (out_tx, _out_rx) = mpsc::channel(1024);
    let (stop_tx, _stop_rx) = watch::channel(false);

    // 1. Verifica se già connesso (come in actor.rs)
    if state.is_user_connected(user_id).await {
        state.force_disconnect_user(user_id).await;
    }

    // 2. Registra connessione
    state.register_connection(user_id, session_id, username.clone(), out_tx.clone(), stop_tx.clone()).await?;

    // 3. Crea/riutilizza canale notifiche
    let _user_tx = state.get_or_create_user_notification_channel(user_id).await;

    // 4. Carica conversazioni (simulando initial_state)
    let conversations: Vec<(String,)> = sqlx::query_as(
        "SELECT c.id
         FROM conversations c
         INNER JOIN participants p ON c.id = p.conversation_id
         WHERE p.user_id = ?
         ORDER BY c.created_at DESC"
    )
        .bind(user_id.to_string())
        .fetch_all(&state.pool)
        .await?;

    // 5. Setup broadcast subscriptions per ogni conversazione
    for (conv_id_str,) in conversations {
        let conv_id = Uuid::parse_str(&conv_id_str)?;
        let _conv_tx = state.get_or_create_broadcast_tx(conv_id).await;
    }

    Ok(())
}

/// Benchmark: singolo login (baseline)
fn bench_single_login(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("single_login", |b| {
        b.iter(|| {
            let (state, users) = rt.block_on(create_test_setup(1));
            let state = Arc::new(state);
            let (user_id, username) = users[0].clone();

            rt.block_on(async {
                simulate_connection(state, user_id, username)
                    .await
                    .unwrap();
            });
        });
    });
}

/// Benchmark: burst di login simultanei
fn bench_concurrent_logins(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("concurrent_logins");

    group.sample_size(10);
    group.measurement_time(std::time::Duration::from_secs(30));

    for num_users in [10, 50, 100, 500, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_users),
            num_users,
            |b, &num_users| {
                b.iter(|| {
                    let (state, users) = rt.block_on(create_test_setup(num_users));
                    let state = Arc::new(state);

                    rt.block_on(async {
                        let mut handles = Vec::new();

                        // Spawn tutti i login contemporaneamente
                        for (user_id, username) in users {
                            let state = Arc::clone(&state);
                            let handle = tokio::spawn(async move {
                                simulate_connection(state, user_id, username)
                                    .await
                                    .unwrap();
                            });
                            handles.push(handle);
                        }

                        // Aspetta che tutti completino
                        for handle in handles {
                            handle.await.unwrap();
                        }
                    });
                });
            }
        );
    }
    group.finish();
}

/// Benchmark: login sequenziali (per confronto)
fn bench_sequential_logins(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("sequential_logins");

    group.sample_size(10);
    group.measurement_time(std::time::Duration::from_secs(20));

    for num_users in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_users),
            num_users,
            |b, &num_users| {
                b.iter(|| {
                    let (state, users) = rt.block_on(create_test_setup(num_users));
                    let state = Arc::new(state);

                    rt.block_on(async {
                        // Login uno alla volta
                        for (user_id, username) in users {
                            simulate_connection(state.clone(), user_id, username)
                                .await
                                .unwrap();
                        }
                    });
                });
            }
        );
    }
    group.finish();
}

/// Benchmark: login con riconnessioni (stesso utente)
fn bench_reconnections(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("reconnections");

    group.sample_size(10);

    for num_reconnects in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_reconnects),
            num_reconnects,
            |b, &num_reconnects| {
                b.iter(|| {
                    let (state, users) = rt.block_on(create_test_setup(1));
                    let state = Arc::new(state);
                    let (user_id, username) = users[0].clone();

                    rt.block_on(async {
                        // Simula lo stesso utente che riconnette N volte
                        for _ in 0..num_reconnects {
                            simulate_connection(state.clone(), user_id, username.clone())
                                .await
                                .unwrap();
                        }
                    });
                });
            }
        );
    }
    group.finish();
}

/// Benchmark: login con force_disconnect (altri device)
fn bench_force_disconnect_scenario(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("force_disconnect");

    group.sample_size(10);

    for num_users in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_users),
            num_users,
            |b, &num_users| {
                b.iter(|| {
                    let (state, users) = rt.block_on(create_test_setup(num_users));
                    let state = Arc::new(state);

                    rt.block_on(async {
                        // Prima connessione - tutti si connettono
                        for (user_id, username) in &users {
                            simulate_connection(state.clone(), *user_id, username.clone())
                                .await
                                .unwrap();
                        }

                        // Seconda connessione - tutti riconnettono (force disconnect)
                        let mut handles = Vec::new();
                        for (user_id, username) in users {
                            let state = Arc::clone(&state);
                            let handle = tokio::spawn(async move {
                                simulate_connection(state, user_id, username)
                                    .await
                                    .unwrap();
                            });
                            handles.push(handle);
                        }

                        for handle in handles {
                            handle.await.unwrap();
                        }
                    });
                });
            }
        );
    }
    group.finish();
}

/// Benchmark: stress test con conversazioni
fn bench_login_with_conversations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("login_with_conversations");

    group.sample_size(10);
    group.measurement_time(std::time::Duration::from_secs(30));

    // Test con numero crescente di conversazioni per utente
    for num_convs in [10, 20, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_convs),
            num_convs,
            |b, &num_convs| {
                b.iter(|| {
                    let (state, users) = rt.block_on(async {
                        let (state, users) = create_test_setup(1000).await;

                        // Crea N conversazioni per ogni utente
                        for (user_id, _) in &users {
                            for i in 0..num_convs {
                                let conv_id = Uuid::new_v4();
                                sqlx::query(
                                    "INSERT INTO conversations (id, kind, title, owner_id, created_at)
                                     VALUES (?, 'group', ?, ?, ?)"
                                )
                                    .bind(conv_id.to_string())
                                    .bind(format!("Conv {}", i))
                                    .bind(user_id.to_string())
                                    .bind(chrono::Utc::now().timestamp())
                                    .execute(&state.pool)
                                    .await
                                    .unwrap();

                                sqlx::query(
                                    "INSERT INTO participants (conversation_id, user_id, role, last_read_sequence)
                                     VALUES (?, ?, 'owner', 0)"
                                )
                                    .bind(conv_id.to_string())
                                    .bind(user_id.to_string())
                                    .execute(&state.pool)
                                    .await
                                    .unwrap();
                            }
                        }

                        (state, users)
                    });
                    let state = Arc::new(state);

                    rt.block_on(async {
                        let mut handles = Vec::new();

                        for (user_id, username) in users {
                            let state = Arc::clone(&state);
                            let handle = tokio::spawn(async move {
                                simulate_connection(state, user_id, username)
                                    .await
                                    .unwrap();
                            });
                            handles.push(handle);
                        }

                        for handle in handles {
                            handle.await.unwrap();
                        }
                    });
                });
            }
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_single_login,
    bench_concurrent_logins,
    bench_sequential_logins,
    bench_reconnections,
    bench_force_disconnect_scenario,
    bench_login_with_conversations
);
criterion_main!(benches);