use std::{env, net::SocketAddr};

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{delete, get, patch, post},
    Json, Router,
};
use dotenv::dotenv;
use serde::{Deserialize, Serialize};
use sqlx::{mysql::MySqlPoolOptions, MySql, Pool, Row};

#[derive(sqlx::FromRow, Debug, Serialize)]
struct User {
    id: i32,
    discord_id: u64,
    user_name: String,
    bucks: i32,
}
#[derive(sqlx::FromRow, Debug, Serialize, Deserialize)]
struct Wager {
    id: i32,
    amount: i32,
    closed: bool,
}

#[derive(sqlx::FromRow, Debug, Serialize)]
struct UserWager {
    id: i32,
    wager_id: i32,
    user_id: i32,
}

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
        .route("/user", get(get_user))
        .route("/user", post(create_user))
        .route("/user/wager", post(add_user_to_wager))
        .route("/user/wager", delete(remove_user_from_wager))
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
struct GetUserParams {
    discord_id: u64,
}
async fn get_user(
    State(pool): State<Pool<MySql>>,
    user_params: Query<GetUserParams>,
) -> (StatusCode, Json<User>) {
    match sqlx::query_as::<_, User>("SELECT * FROM user where discord_id = ?")
        .bind(user_params.discord_id)
        .fetch_optional(&pool)
        .await
        .expect("expected user query to succeed")
    {
        Some(user) => (StatusCode::OK, Json(user)),
        None => (
            StatusCode::NOT_FOUND,
            Json(User {
                id: 0,
                discord_id: 0,
                user_name: "".to_string(),
                bucks: 0,
            }),
        ),
    }
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
#[derive(Deserialize, Debug)]
struct CloseWagerPayload {
    wager_id: i32,
    winning_user_discord_ids: Vec<u64>,
    losing_user_discord_ids: Vec<u64>,
}
#[derive(Serialize)]
struct SucessfullCloseWager {
    winners: Vec<User>,
    losers: Vec<User>,
    wager: Wager,
}

async fn close_wager(
    State(pool): State<Pool<MySql>>,
    Json(payload): Json<CloseWagerPayload>,
) -> (StatusCode, Json<SucessfullCloseWager>) {
    // get the wager
    let wager: Option<Wager> = sqlx::query_as::<_, Wager>("SELECT * FROM wager WHERE id = ?")
        .bind(payload.wager_id)
        .fetch_optional(&pool)
        .await
        .expect("expected wager query to succeed");

    if wager.is_none() {
        return (
            StatusCode::EXPECTATION_FAILED,
            Json(SucessfullCloseWager {
                winners: vec![],
                losers: vec![],
                wager: Wager {
                    id: 0,
                    amount: 0,
                    closed: false,
                },
            }),
        );
    }

    let wager = wager.unwrap();

    if wager.closed {
        return (
            StatusCode::EXPECTATION_FAILED,
            Json(SucessfullCloseWager {
                winners: vec![],
                losers: vec![],
                wager,
            }),
        );
    }

    // get the users in the wager
    let user_ids: Vec<u8> =
        sqlx::query_as::<_, UserWager>("SELECT * FROM user_wager WHERE wager_id = ?")
            .bind(payload.wager_id)
            .fetch_all(&pool)
            .await
            .expect("expected user_wager query to succeed")
            .into_iter()
            .map(|user_wager| user_wager.user_id as u8)
            .collect();

    let mut users: Vec<User> = vec![];
    for user in &user_ids {
        let user = sqlx::query_as::<_, User>("SELECT * FROM user WHERE id = ?")
            .bind(user)
            .fetch_one(&pool)
            .await
            .expect("expected user query to succeed");
        users.push(user);
    }

    // the users in the payload for winners and losers should be checked to make sure they were
    // added to the wager. If they were not, they should be ignored
    let mut winning_users: Vec<User> = vec![];
    let mut losing_users: Vec<User> = vec![];
    for user in users {
        if payload.winning_user_discord_ids.contains(&user.discord_id) {
            winning_users.push(user);
        } else if payload.losing_user_discord_ids.contains(&user.discord_id) {
            losing_users.push(user);
        }
    }

    //payout the winners
    let mut payout = 0;
    //print out winning users
    if winning_users.len() > 0 {
        payout = wager.amount * (losing_users.len() as i32) / winning_users.len() as i32;
    }
    for user in &winning_users {
        let new_bucks = user.bucks + payout;
        sqlx::query("UPDATE user SET bucks = ? WHERE id = ?")
            .bind(new_bucks)
            .bind(user.id)
            .execute(&pool)
            .await
            .expect("expected user update to succeed");
    }

    //take money from the losers
    // if they do not have enough money, put them at zero
    for user in &losing_users {
        let mut new_bucks = user.bucks - wager.amount;
        if new_bucks < 0 {
            new_bucks = 0;
        }
        sqlx::query("UPDATE user SET bucks = ? WHERE id = ?")
            .bind(new_bucks)
            .bind(user.id)
            .execute(&pool)
            .await
            .expect("expected user update to succeed");
    }

    //mark the wager as closed
    sqlx::query("UPDATE wager SET closed = true WHERE id = ?")
        .bind(payload.wager_id)
        .execute(&pool)
        .await
        .expect("expected wager update to succeed");

    (
        StatusCode::OK,
        Json(SucessfullCloseWager {
            winners: winning_users,
            losers: losing_users,
            wager,
        }),
    )
}

#[derive(Deserialize)]
struct WagerInput {
    amount: i32,
}

async fn create_wager(
    State(pool): State<Pool<MySql>>,
    Json(payload): Json<WagerInput>,
) -> (StatusCode, Json<Wager>) {
    // create a wager and return the id of the wager
    let wager_id = sqlx::query("INSERT INTO wager(amount) VALUES (?)")
        .bind(payload.amount)
        .execute(&pool)
        .await
        .expect("excpect wager to successfull be created")
        .last_insert_id();

    // return the wager id
    (
        StatusCode::OK,
        Json(Wager {
            id: wager_id as i32,
            amount: payload.amount,
            closed: false,
        }),
    )
}
#[derive(Deserialize, Serialize)]
struct UserForWagerPayload {
    discord_id: u64,
    user_name: String,
    wager_id: i32,
}
async fn add_user_to_wager(
    State(pool): State<Pool<MySql>>,
    Json(payload): Json<UserForWagerPayload>,
) -> (StatusCode, Json<UserForWagerPayload>) {
    // get the user id from the discord id
    // if the user does not exist, create them with an amount of 500
    let user_id: i32 = match sqlx::query("SELECT id from user where discord_id = ?")
        .bind(payload.discord_id)
        .fetch_optional(&pool)
        .await
        .expect("expected user query to succeed")
        .map(|row| row.try_get("id").expect("expected user id to be an i32"))
    {
        Some(id) => id,
        None => sqlx::query("INSERT INTO user(discord_id, user_name, bucks) VALUES (?, ?, ?)")
            .bind(payload.discord_id)
            .bind(&payload.user_name)
            .bind(500)
            .execute(&pool)
            .await
            .expect("expected user insert to succeed")
            .last_insert_id() as i32,
    };

    //check if user is already in the wager
    if sqlx::query("SELECT * FROM user_wager WHERE user_id = ? AND wager_id = ?")
        .bind(user_id)
        .bind(payload.wager_id)
        .fetch_optional(&pool)
        .await
        .expect("expected user_wager query to succeed")
        .is_some()
    {
        return (StatusCode::OK, Json(payload));
    }

    //check if user has enough money to join wager
    let user = sqlx::query_as::<_, User>("SELECT * FROM user WHERE id = ?")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("expected user query to succeed");
    let wager = sqlx::query_as::<_, Wager>("SELECT * FROM wager WHERE id = ?")
        .bind(payload.wager_id)
        .fetch_one(&pool)
        .await
        .expect("expected wager query to succeed");
    if user.bucks < wager.amount {
        return (StatusCode::BAD_REQUEST, Json(payload));
    }

    // insert the user into the wager
    sqlx::query("INSERT INTO user_wager(user_id, wager_id) VALUES (?, ?)")
        .bind(user_id)
        .bind(payload.wager_id)
        .execute(&pool)
        .await
        .expect("expected user_wager query to succeed");

    return (StatusCode::OK, Json(payload));
}
#[derive(Deserialize, Serialize)]
struct RemoveUserWagerPayload {
    discord_id: u64,
    wager_id: i32,
}
async fn remove_user_from_wager(
    State(pool): State<Pool<MySql>>,
    Json(payload): Json<RemoveUserWagerPayload>,
) -> (StatusCode, Json<RemoveUserWagerPayload>) {
    //check if user is in the wager. If they are, remove them
    if let Some(user_wager) = sqlx::query_as::<_, UserWager>(
        "SELECT * FROM user_wager WHERE user_id = ? AND wager_id = ?",
    )
    .bind(payload.discord_id)
    .bind(payload.wager_id)
    .fetch_optional(&pool)
    .await
    .expect("expected user_wager query to succeed")
    {
        sqlx::query("DELETE FROM user_wager WHERE id = ?")
            .bind(user_wager.id)
            .execute(&pool)
            .await
            .expect("expected user_wager delete to succeed");
        return (StatusCode::OK, Json(payload));
    } else {
        return (StatusCode::BAD_REQUEST, Json(payload));
    }
}
