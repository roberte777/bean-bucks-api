use std::{borrow::Borrow, env, net::SocketAddr};

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, patch, post},
    Json, Router,
};
use dotenv::dotenv;
use serde::{Deserialize, Serialize};
use sqlx::{mysql::MySqlPoolOptions, MySql, Pool};

#[derive(sqlx::FromRow, Debug, Serialize)]
struct User {
    id: i32,
    discord_id: u64,
    user_name: String,
    bucks: i32,
}
struct Wager {
    id: i32,
    amount: i32,
}

struct UserWager {
    id: i32,
    wager_id: i32,
    user_id: i32,
}
// etc.

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    dotenv().ok();
    let database_url = env::var("DATABASE_URL").expect("Database URL should exist");
    let pool = MySqlPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    let app = Router::new()
        .route("/", get(root))
        .route("/users", get(list_users))
        .route("/user", get(create_user))
        .route("/user/wager", post(add_user_to_wager))
        .route("/wager", post(create_wager))
        .route("/wager", patch(close_wager))
        .with_state(pool);
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}
// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}

async fn list_users(State(pool): State<Pool<MySql>>) -> (StatusCode, Json<Vec<User>>) {
    let rows: Vec<User> = sqlx::query_as::<_, User>("SELECT * FROM user")
        .fetch_all(&pool)
        .await
        .expect("expected user query to success");
    (StatusCode::OK, Json(rows))
}

#[derive(Deserialize)]
struct CreateUser {
    discord_id: u64,
    user_name: String,
}

async fn create_user(
    State(pool): State<Pool<MySql>>,
    Json(payload): Json<CreateUser>,
) -> (StatusCode, String) {
    //search database for discord_id
    let user = sqlx::query("SELECT id from user where discord_id = ?")
        .bind(payload.discord_id)
        .fetch_optional(&pool)
        .await
        .expect("expected user query to succeed");

    //if user already exists, return status code
    if let Some(_) = user {
        return (StatusCode::CONFLICT, "user already exists".to_string());
    }
    //create user
    let res = sqlx::query("INSERT INTO user(discord_id, user_name, bucks) VALUES (?, ?, ?)")
        .bind(payload.discord_id)
        .bind(payload.user_name)
        .bind(500)
        .execute(&pool)
        .await;
    match res {
        Ok(_) => (StatusCode::OK, "user created".to_string()),
        Err(e) => (
            StatusCode::EXPECTATION_FAILED,
            format!("failed to create user: {}", e.to_string()),
        ),
    }
}

/*
 * This method should do several things
* First, it should verify the wager the user is trying to close is not already
* closed.
* Then, it should take the appropriate amount of money from the losing users
* if they do not have enough money, put them at zero
* It should then give money to the winning users
* Finally, it should mark the wager as closed
 */
async fn close_wager() {
    todo!()
}

async fn create_wager() {
    todo!()
}
async fn add_user_to_wager() {
    todo!()
}
