use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use server::{
    state::AppState,
    web_socket::handlers::message::handle_chat_message,
};
use serde_json::json;
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;
use tokio::runtime::Runtime;

async fn create_test_setup(num_users: usize) -> (AppState, Uuid, Vec<(Uuid, String)>) {
    let pool = SqlitePool::connect(":memory:")
        .await
        .expect("Failed to create test database");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    let state = AppState::new(pool.clone(), "test_secret".to_string());

    let mut users = Vec::new();

    for i in 0..num_users {
        let username = format!("user_{}", i);
        let user_id = Uuid::new_v4();

        let user_tx = state.get_or_create_user_notification_channel(user_id).await;
        let mut user_rx = user_tx.subscribe();
        tokio::spawn(async move {
            while user_rx.recv().await.is_ok() {}
        });

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

    let conv_id = Uuid::new_v4();
    let owner = users[0].0;

    sqlx::query(
        "INSERT INTO conversations (id, kind, title, owner_id, created_at)
         VALUES (?, 'group', 'Test Group', ?, ?)"
    )
        .bind(conv_id.to_string())
        .bind(owner.to_string())
        .bind(chrono::Utc::now().timestamp())
        .execute(&pool)
        .await
        .expect("Failed to create conversation");

    for (user_id, _) in &users {
        sqlx::query(
            "INSERT INTO participants (conversation_id, user_id, role, last_read_sequence)
             VALUES (?, ?, 'member', 0)"
        )
            .bind(conv_id.to_string())
            .bind(user_id.to_string())
            .execute(&pool)
            .await
            .expect("Failed to add participant");
    }

    let conv_tx = state.get_or_create_broadcast_tx(conv_id).await;
    let mut conv_rx = conv_tx.subscribe();
    tokio::spawn(async move {
        while conv_rx.recv().await.is_ok() {}
    });

    (state, conv_id, users)
}

fn bench_single_message(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("single_message", |b| {
        b.iter(|| {
            let (state, conv_id, users) = rt.block_on(create_test_setup(2));

            rt.block_on(async {
                let mut msg = json!({
                    "type": "chat_message",
                    "conversation_id": conv_id.to_string(),
                    "content": "test",
                    "client_msg_id": Uuid::new_v4().to_string()
                });

                handle_chat_message(&state, &mut msg, users[0].0, &users[0].1)
                    .await
                    .unwrap();
            });
        });
    });
}

fn bench_burst(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("burst_messages");

    group.sample_size(10);

    // ✨ Aumenta timeout per test lenti
    group.measurement_time(std::time::Duration::from_secs(20));  // Default è 5s


    for size in [10, 50, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                let (state, conv_id, users) = rt.block_on(create_test_setup(size));
                let state = Arc::new(state);

                rt.block_on(async {
                    let mut handles = Vec::new();

                    for (user_id, username) in users {
                        let state = Arc::clone(&state);
                        let handle = tokio::spawn(async move {
                            let mut msg = json!({
                                "type": "chat_message",
                                "conversation_id": conv_id.to_string(),
                                "content": format!("burst from {}", username),
                                "client_msg_id": Uuid::new_v4().to_string()
                            });

                            handle_chat_message(&*state, &mut msg, user_id, &username)
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
        });
    }
    group.finish();
}

fn bench_sequential(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("sequential_messages");

    group.sample_size(10);

    for num_messages in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_messages),
            num_messages,
            |b, &num_messages| {
                b.iter(|| {
                    let (state, conv_id, users) = rt.block_on(create_test_setup(10));

                    rt.block_on(async {
                        for i in 0..num_messages {
                            let user_idx = i % users.len();
                            let (user_id, username) = &users[user_idx];

                            let mut msg = json!({
                                "type": "chat_message",
                                "conversation_id": conv_id.to_string(),
                                "content": format!("msg {}", i),
                                "client_msg_id": Uuid::new_v4().to_string()
                            });

                            handle_chat_message(&state, &mut msg, *user_id, username)
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

criterion_group!(benches, bench_single_message, bench_burst, bench_sequential);
criterion_main!(benches);