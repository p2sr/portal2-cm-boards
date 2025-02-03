use crate::{models::{changelog::MapScoreDate, points::*, users::*}, tools::error::{ServerError, ErrorType}};
use sqlx::PgPool;

impl Users {
    // TODO: Testing for this
    // TODO: Fix edge case parsing for steam user.
    /// Fetch a [Users] from the official Steam API.
    pub async fn new_from_steam(steam_api_key: &str, profile_number: &str) -> Result<Users, ServerError> {
        // http://steamcommunity.com/profiles/{}/?xml=1
        // GET https://api.steampowered.com/ISteamUser/GetPlayerSummaries/v2/
        let steam_api_url = format!(
            "https://api.steampowered.com/ISteamUser/GetPlayerSummaries/v2/?key={}&steamids={}",
            steam_api_key, profile_number
        );
        let user = reqwest::get(&steam_api_url)
            .await?
            .json::<GetPlayerSummariesWrapper>()
            .await?;
        println!("{user:#?}");
        if user.response.players.is_empty() {
            return Err(ServerError {
                error_message: format!("User not found"),
                error_type: ErrorType::Reqwest,
            });
        }
        Ok(Users {
            profile_number: profile_number.to_string(),
            board_name: None,
            steam_name: Some(user.response.players[0].personaname.clone()),
            banned: false,
            registered: 0,
            avatar: Some(user.response.players[0].avatarfull.clone()),
            ..Default::default()
        })
    }
    /// Returns a [Users] from the given `profile_number`.
    #[allow(dead_code)]
    pub async fn get_user(pool: &PgPool, profile_number: String) -> Result<Option<Users>, sqlx::Error> {
        sqlx::query_as::<_, Users>(r#"SELECT * FROM users WHERE profile_number = $1"#)
            .bind(profile_number)
            .fetch_optional(pool)
            .await
    }
    /// Gets a [UsersPage] from a given `profile_number`.
    /// 
    /// Will favor `board_name` over `steam_name`.
    pub async fn get_user_data(pool: &PgPool, profile_number: &str) -> Result<Option<UsersPage>, sqlx::Error> {
        sqlx::query_as::<_, UsersPage>(
            r#"
                SELECT COALESCE(board_name, steam_name) AS user_name, avatar
                    FROM users
                    WHERE users.profile_number = $1
                "#,
        )
        .bind(profile_number)
        .fetch_optional(pool)
        .await
    }
    // TODO: There are faster ways to do this. <-----
    /// Pattern match on a given string to find similar names (supports board/steam names).
    pub async fn check_board_name(pool: &PgPool, nick_name: &str) -> std::result::Result<Vec<String>, sqlx::Error> {
        // Limitation to how SQLX inserts strings.
        let nick_name = format!("%{}%", &nick_name);
        sqlx::query_scalar(
            r#"
                SELECT users.profile_number FROM users
                WHERE 
                    CASE
                        WHEN users.board_name IS NULL
                            THEN LOWER(users.steam_name) LIKE LOWER($1)
                        WHEN users.board_name IS NOT NULL
                            THEN LOWER(users.board_name) LIKE LOWER($1)
                    END
                "#,
        )
        .bind(nick_name)
        .fetch_all(pool)
        .await
    }
    /// Returns a list of all banned player's `profile_numbers`.
    pub async fn get_banned(pool: &PgPool) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar(r#"SELECT users.profile_number FROM users WHERE users.banned = True"#)
            .fetch_all(pool)
            .await
    }
    /// Returns a list of all banned player's as a [UsersDisplay].
    pub async fn get_banned_display(pool: &PgPool) -> Result<Vec<UsersDisplay>, sqlx::Error> {
        sqlx::query_as::<_, UsersDisplay>(
            r#" SELECT users.profile_number,
            COALESCE(users.board_name, users.steam_name) as user_name, 
            users.avatar
                FROM users WHERE users.banned = 'true'"#,
        )
        .fetch_all(pool)
        .await
    }
    /// Returns the boolean flag associated with the user in the boards, if Err, assumed User does not exist.
    pub async fn check_banned(pool: &PgPool, profile_number: &str) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar(r#"SELECT users.banned FROM users WHERE users.profile_number = $1"#)
            .bind(profile_number)
            .fetch_one(pool)
            .await
    }
    #[allow(dead_code)]
    /// Returns the title associated with the user.
    pub async fn get_title(pool: &PgPool, profile_number: String) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar(r#"SELECT title FROM users WHERE users.profile_number = $1"#)
            .bind(profile_number)
            .fetch_optional(pool)
            .await
    }
    #[allow(dead_code)]
    /// Returns a [Socials] associated with a given user's `profile_number`.
    pub async fn get_socials(pool: &PgPool, profile_number: String) -> Result<Option<Socials>, sqlx::Error> {
        sqlx::query_as::<_, Socials>(
            r#"
            SELECT twitch, youtube, discord_id 
            FROM users 
            WHERE profile_number = $1"#,
        )
        .bind(profile_number)
        .fetch_optional(pool)
        .await
    }
    #[allow(dead_code)]
    /// Returns the admin information associated with the user.
    pub async fn get_admin_for_user(pool: &PgPool, profile_number: String) -> Result<Option<i32>, sqlx::Error> {
        sqlx::query_scalar(r#"SELECT admin FROM users WHERE profile_number = $1"#)
            .bind(profile_number)
            .fetch_optional(pool)
            .await
    }
    /// Returns [UsersDisplay] for all admins.
    /// Usage:  admin_value = 0     -> Non-admin user
    ///         admin_value = 1     -> Standard admin
    ///         admin_value = 2     -> Shadow admin
    ///             (Has admin permissions, is not publically listed)
    ///             (Typically reserved for former admins, trusted players)
    ///         admin_value = 3     -> Developer admin
    ///             (Has admin permissions as an active developer only)
    pub async fn get_all_admins(
        pool: &PgPool,
        admin_value: i32,
    ) -> Result<Vec<UsersDisplay>, sqlx::Error> {
            sqlx::query_as::<_, UsersDisplay>(
                r#"
                SELECT users.profile_number, 
                    COALESCE(users.board_name, users.steam_name) AS user_name,
                    users.avatar
                FROM users
                WHERE users.admin = $1
                "#,
            )
            .bind(admin_value)
            .fetch_all(pool)
            .await
    }
    /// Returns all users that have donated to the board. Ordered by highest amount.
    pub async fn get_donators(pool: &PgPool) -> Result<Vec<Users>, sqlx::Error> {
        sqlx::query_as::<_, Users>(
            r#"
        SELECT * FROM users
            WHERE donation_amount IS NOT NULL
            ORDER BY CAST(donation_amount AS decimal) DESC;"#,
        )
        .fetch_all(pool)
        .await
    }
    /// Returns a [ProfileData] for the given `profile_number`.
    pub async fn get_profile(pool: &PgPool, profile_number: &String) -> Result<ProfileData, sqlx::Error> {
        let s1 = r#"SELECT old.steam_id AS map, old.name AS map_name, old.score, old.timestamp FROM 
            (SELECT maps.steam_id, maps.name, changelog.score, changelog.timestamp FROM maps 
            INNER JOIN changelog ON (maps.steam_id = changelog.map_id) WHERE changelog.timestamp = (
            SELECT *
                FROM (
                    SELECT "#;
        let s2 = r#"(o1.timestamp)
        FROM
        (SELECT DISTINCT ON (m1.steam_id) m1.steam_id, m1.name, cl1.score, cl1.timestamp, cl1.id
            FROM changelog AS cl1
            INNER JOIN maps AS m1
                ON (cl1.map_id = m1.steam_id)
            INNER JOIN chapters AS c1
                ON (m1.chapter_id = c1.id)
            WHERE cl1.profile_number = $1
            AND c1.is_multiplayer = $2
            AND cl1.banned = 'false'
            AND cl1.verified = 'true'
            AND cl1.category_id = m1.default_cat_id
            ORDER BY m1.steam_id, cl1.score) AS o1) AS a)) AS old;"#;

        let oldest_sp = sqlx::query_as::<_, MapScoreDate>(&format!("{}{}{}", s1, "MIN", s2))
            .bind(profile_number)
            .bind(false)
            .fetch_one(pool)
            .await?;
        let newest_sp = sqlx::query_as::<_, MapScoreDate>(&format!("{}{}{}", s1, "MAX", s2))
            .bind(profile_number)
            .bind(false)
            .fetch_one(pool)
            .await?;
        let oldest_coop = sqlx::query_as::<_, MapScoreDate>(&format!("{}{}{}", s1, "MIN", s2))
            .bind(profile_number)
            .bind(true)
            .fetch_one(pool)
            .await?;
        let newest_coop = sqlx::query_as::<_, MapScoreDate>(&format!("{}{}{}", s1, "MAX", s2))
            .bind(profile_number)
            .bind(true)
            .fetch_one(pool)
            .await?;
        Ok(ProfileData {
            oldest_sp,
            newest_sp,
            oldest_coop,
            newest_coop,
        })
    }
    // TODO: Consider using profanity filter (only for really bad names): https://docs.rs/censor/latest/censor/
    /// Inserts a new user into the database from a given [Users]. Returns the [Users] object.
    pub async fn insert_new_users(pool: &PgPool, new_user: Users) -> Result<Users, sqlx::Error> {
        // let mut res = String::new();
        // We do not care about the returning profile_number. As it is not generated and we already have it
        sqlx::query_as::<_, Users>(
            r#"
                INSERT INTO Users
                (profile_number, board_name, steam_name, banned, registered, 
                avatar, twitch, youtube, title, admin, donation_amount, discord_id)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                RETURNING *"#,
        )
        .bind(new_user.profile_number)
        .bind(new_user.board_name)
        .bind(new_user.steam_name)
        .bind(new_user.banned)
        .bind(new_user.registered)
        .bind(new_user.avatar)
        .bind(new_user.twitch)
        .bind(new_user.youtube)
        .bind(new_user.title)
        .bind(new_user.admin)
        .bind(new_user.donation_amount)
        .bind(new_user.discord_id)
        .fetch_one(pool)
        .await
    }
    #[allow(dead_code)]
    /// Update a given user with a new [Users] object.
    pub async fn update_existing_user(pool: &PgPool, updated_user: Users) -> Result<Users, sqlx::Error> {
        // TODO: Check to make sure user has correct AUTH to update specific items
        // (board_name should only be changed by the backend, admin should only be updated by admin etc)
        sqlx::query_as::<_, Users>(
            r#"
                UPDATE users
                SET board_name = $1, steam_name = $2, banned = $3, registered = $4, 
                avatar = $5, twitch = $6, youtube = $7, title = $8, admin = $9,
                donation_amount = $10, discord_id = $11
                WHERE profile_number = $12 RETURNING *"#,
        )
        .bind(updated_user.board_name)
        .bind(updated_user.steam_name)
        .bind(updated_user.banned)
        .bind(updated_user.registered)
        .bind(updated_user.avatar)
        .bind(updated_user.twitch)
        .bind(updated_user.youtube)
        .bind(updated_user.title)
        .bind(updated_user.admin)
        .bind(updated_user.donation_amount)
        .bind(updated_user.discord_id)
        .bind(updated_user.profile_number)
        .fetch_one(pool)
        .await
    }
    /// Returns the **PREVIOUS** `avatar` after updating.
    pub async fn update_avatar(
        pool: &PgPool,
        profile_number: &str,
        avatar: &str,
    ) -> Result<String, sqlx::Error> {
        sqlx::query_scalar(
            r#"WITH old AS (
                SELECT avatar FROM users WHERE profile_number = $2
            )
            UPDATE users SET avatar = $1 
                WHERE profile_number = $2 RETURNING (SELECT avatar FROM old)"#,
        )
        .bind(avatar)
        .bind(profile_number)
        .fetch_one(pool)
        .await
    }
    #[allow(dead_code)]
    /// Deletion for a given `profile_number`.
    pub async fn delete_user(pool: &PgPool, profile_number: String) -> Result<Users, sqlx::Error> {
        sqlx::query_as::<_, Users>(
            r#"DELETE FROM users 
                WHERE profile_number = $1 RETURNING *"#,
        )
        .bind(profile_number)
        .fetch_one(pool)
        .await
    }
}
