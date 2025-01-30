use crate::models::changelog::{Changelog, ChangelogInsert, SubmissionChangelog};
use crate::models::demos::*;
use crate::models::maps::Maps;
use crate::tools::cache::CacheState;
use crate::tools::config::Config;
use crate::tools::helpers::get_valid_changelog_insert;
use actix_multipart::Multipart;
use actix_web::{delete, get, post, web, HttpResponse, Responder};
use anyhow::{bail, Result};
use futures::{StreamExt, TryStreamExt};
use raze::api::*;
use raze::utils::*;
use sqlx::PgPool;
use std::fs::remove_file;
use std::fs::OpenOptions;
use std::io::Write;
use std::str;

/// GET endpoint to return demo information.
/// ## Expects **one** of following fields:
///
/// **Required Parameters**: cl_id, demo_id
///
/// ## Parameters:
///
/// - **cl_id**    
///     - `i64`: ID for a changelog entry, will grab the most updated demo assocaited with that changelog entry.
/// - **demo_id**
///     - `i64`: ID for a specific demo (less likely to be what you want).
///
/// ## Example endpoints:       
/// - `/api/v1/demos?cl_id=15625`
/// - `/api/v1/demos?demo_id=12651`
///
///[get("/demos")]
///pub async fn demos(pool: web::Data<PgPool>, query: web::Query<DemoOptions>) -> impl Responder {
///    let query = query.into_inner();
///    let res_str = "Could not find demo.";
///    if query.demo_id.is_some() & query.cl_id.is_none() {
///        match Demos::get_demo(pool.get_ref(), query.demo_id.unwrap()).await {
///            Ok(Some(demo)) => HttpResponse::Ok().json(demo),
///            Err(e) => {
///                eprintln!("{}", e);
///                HttpResponse::NotFound().body(res_str)
///            }
///            _ => HttpResponse::NotFound().body(res_str),
///        }
///    } else if query.demo_id.is_none() & query.cl_id.is_some() {
///        match Demos::get_demo_by_cl_id(pool.get_ref(), query.cl_id.unwrap()).await {
///            Ok(Some(demo)) => HttpResponse::Ok().json(demo),
///            Err(e) => {
///                eprintln!("{}", e);
///                HttpResponse::NotFound().body(res_str)
///            }
///            _ => HttpResponse::NotFound().body(res_str),
///        }
///    } else {
///        HttpResponse::BadRequest()
///            .body("Neither a `cl_id` nor a `demo_id` was provided to search on.")
///    }
///}
///
////// POST endpoint to upload a new demo changelog entry. Returns the new demo ID.
//////
////// ## Note: **DOES NOT HANDLE ACTUAL DEMO FILES**
//////
////// ## Parameters:
////// - `file_id`           
//////     - **Required** - `String` : ID for the player.
////// - `cl_id`
//////     - **Required** - `i64` : The associated changelog entry ID.
////// - `parsed_successfully`
//////     - **Required** - `bool` : If the demo was successfully parsed, outside posts should be false.
////// - `partner_name`           
//////     - **Optional** - `String` : Name of the partner (used for legacy demo reasons)
////// - `sar_version`           
//////     - **Optional** - `String` : Version of SAR used.
//////
////// ## Example endpoint:       
////// - `/api/v1/demos`
//////
////// Makes a call to the underlying [Demos::insert_demo]
//////
////// ## Example JSON input string:
////// ```json
////// {
//////     "file_id": "TripleLaser_1053_76561198003223063_1.dem",
//////     "partner_name": null,
//////     "parsed_successfully": true,
//////     "sar_version": null,
//////     "cl_id": 8513,
//////     "updated": null
////// }
////// ```
//////
////// ## Example JSON input response:
////// ```json
////// 1252
////// ```
///#[post("/demos")]
///pub async fn demos_add(pool: web::Data<PgPool>, demo: web::Json<DemoInsert>) -> impl Responder {
///    match Demos::insert_demo(pool.get_ref(), demo.into_inner()).await {
///        Ok(demo_id) => HttpResponse::Ok().json(demo_id),
///        Err(e) => {
///            eprintln!("Error uploading demo -> {e}");
///            HttpResponse::InternalServerError().body("Could not add new demo")
///        }
///    }
///}
///
/////  a. Handle renaming/db interactions (update demo table/specific time that is being uploaded)
/////  b. Pass to backblaze
/////  c. Look to see if there is anything special needed for auto-submit
/////  d. Integrate Parsing
///// Code Reference: https://github.com/Ujang360/actix-multipart-demo/blob/main/src/main.rs
///// TODO: Allow for sar version or partner name?
////// Accepts field values for both a changelog, and a demo file.
////// ## Expects the following fields:
//////
////// **Required Parameters**: timestamp, profile_number, score, map_id
//////
////// **Optional Parameters**: youtube_id, note, cat_id
//////
////// ## Parameters:
//////
////// - **timestamp**    
//////     - `String`: `%Y-%m-%d %H:%M:%S` (use `%20` to denote a space)
////// - **profile_number**
//////     - `String`: Steam ID Number
////// - **score**         
//////     - `i32`: Current board time format         
////// - **map_id**       
//////     - `String`: Steam ID for the map
////// - **youtube_id**
//////     - `String`: Youtube URL Extension.
////// - **note**          
//////     - `String`: Note for the run
////// - **category_id**   
//////     - `i32`: ID for the category being played  
////// - `game_id`
//////     - **Optional** - `i32` : The ID for the game, defaults to the base game (id = 1).
//////
////// ## Example endpoints:       
////// - `/api/v1/demos/changelog?timestamp=2020-08-18%2024:60:60&profile_number=76561198040982247&score=1763&map_id=47763`
//////
///#[post("/demos/changelog")]
///pub async fn demos_changelog(
///    mut payload: Multipart,
///    config: web::Data<Config>,
///    query: web::Query<SubmissionChangelog>,
///    cache: web::Data<CacheState>,
///    pool: web::Data<PgPool>,
///) -> impl Responder {
///    // This function heavily utilizes helper functions to make error propagation easier, and reduce the # of match arms
///    let config = config.into_inner();
///    let mut file_name = String::default();
///    let changelog_insert = match get_valid_changelog_insert(pool.get_ref(), &config, &cache.into_inner(), query.into_inner()).await {
///        Ok(insert) => insert,
///        Err(e) => {
///            eprintln!("Error validating changelog -> {e}");
///            return HttpResponse::UnprocessableEntity().body("Could not validate changelog entry.")
///        }
///    };
///    match parse_and_write_multipart(&mut payload, &mut file_name).await {
///        Ok(_) => (),
///        Err(e) => {
///            eprintln!("Error parsing or writing the file. -> {}", e);
///            return HttpResponse::BadRequest().body("Error parsing or write the file.");
///        }
///    }
///    // Add Changelog/Demo entries to database.
///    match add_to_database(pool.get_ref(), changelog_insert, &config, &file_name, true).await {
///        Ok((cl_id, demo_id)) => HttpResponse::Ok().json((cl_id, demo_id)),
///        Err(e) => {
///            eprintln!("Error with adding changelog/demo insert -> {}", e);
///            HttpResponse::InternalServerError()
///                .body("Failed updating demo/changelog entries to database.")
///        }
///    }
///}
///
///// Different demo entries can have the same changelog ID, but a changelog entry should only have the most recent, valid demo_id.
////// DELETE endpoint to remove a demo from both backbalze and the database.
////// ## Expects **one** of the two parametes
//////
////// ***Note***: If both, or neither parameter is provided you will encounter errors.
////// If you want to delete the demo associated with a changelog entry, use the changelog entry.
//////
////// Parameters: demo_id, cl_id
//////
////// ## Parameters:
//////
////// - **demo_id**    
//////     - `i64`: ID for a demo entry in the db, use this if you want to delete a specifc demo.
////// - **cl_id**
//////     - `i64`: ID for a changelog entry, use this if you want to delete the demo associated with a changelog entry.
//////
////// ## Example endpoints:       
////// - `/api/v1/demos?cl_id=15625`
////// - `/api/v1/demos?demo_id=12651`
///#[delete("/demos")]
///pub async fn demos_delete(
///    query: web::Query<DemoOptions>,
///    config: web::Data<Config>,
///    pool: web::Data<PgPool>,
///) -> impl Responder {
///    let query = query.into_inner();
///    let (cl, demo_id) = match get_changelog_and_demo_id(query, pool.get_ref()).await {
///        Ok((cl, demo_id)) => (cl, demo_id),
///        Err(e) => {
///            eprintln!("{}", e);
///            return HttpResponse::NotFound()
///                .body("Cannot find changelog and demo associated with provided information");
///        }
///    };
///    match delete_demo_file(pool.get_ref(), &config.into_inner(), cl, demo_id).await {
///        Ok(_) => match delete_demo_db(pool.get_ref(), demo_id).await {
///            Ok(_) => HttpResponse::Ok().body("Demo file and entry succesfully removed."),
///            Err(e) => {
///                eprintln!("{}", e);
///                HttpResponse::InternalServerError().body("Error deleting demo entry from database")
///            }
///        },
///        Err(e) => {
///            eprintln!("{}", e);
///            HttpResponse::InternalServerError().body("Error deleting file from backblaze.")
///        }
///    }
///}
///
////// Adds a demo and changelog insert to the database.
//////
////// The debug value passed will remove the added changelog/demo entries inserted, and skip uploading the file for quicker debugging.
///async fn add_to_database(
///    pool: &PgPool,
///    changelog_insert: ChangelogInsert,
///    config: &Config,
///    file_name: &str,
///    debug: bool,
///) -> Result<(i64, i64)> {
///    let mut demo_insert = DemoInsert::default();
///    let cl_id = Changelog::insert_changelog(pool, changelog_insert).await?;
///    demo_insert.cl_id = cl_id;
///    // TODO: How do we want demo files named?
///    let file_id = if !debug {
///        upload_demo(config, file_name).await?
///    } else {
///        Some(format!("{}.dem", file_name))
///    };
///    // Delete Demo
///    remove_file(format!("./demos/{}", file_name))?;
///    if let Some(file_id) = file_id {
///        demo_insert.file_id = file_id;
///    }
///    // Add demo entry to database.
///    let demo_id = Demos::insert_demo(pool, demo_insert).await?;
///    // Update changelog to have the new demo_id
///    Changelog::update_demo_id_in_changelog(pool, cl_id, demo_id).await?;
///    if debug {
///        Changelog::delete_changelog(pool, cl_id).await?;
///        Demos::delete_demo(pool, demo_id).await?;
///    }
///    Ok((cl_id, demo_id))
///}
///
////// Helper function that handles parsing the multipart and writing the file out locally
///async fn parse_and_write_multipart(payload: &mut Multipart, file_name: &mut String) -> Result<()> {
///    while let Ok(Some(mut field)) = payload.try_next().await {
///        let mut content_data = Vec::new();
///        while let Some(Ok(chunk)) = field.next().await {
///            content_data.extend(chunk);
///        }
///        let fname = field.content_disposition().get_filename();
///
///        if let Some(fname) = fname {
///            use std::fs;
///            fs::create_dir_all("./demos")?;
///            let mut file = OpenOptions::new()
///                .create(true)
///                .write(true)
///                .open(format!("./demos/{}", fname))?;
///            file.write_all(&content_data)?;
///            *file_name = fname.to_string();
///            // TODO: Parse Demo
///        }
///    }
///    Ok(())
///}
///
////// Returns a client, and an authenticated session for use with backblaze.
///async fn b2_client_and_auth(config: &Config) -> Result<(reqwest::Client, B2Auth)> {
///    let client = reqwest::ClientBuilder::new().build()?;
///    let auth = b2_authorize_account(
///        &client,
///        format!("{}:{}", config.backblaze.keyid, config.backblaze.key),
///    )
///    .await
///    .unwrap();
///    Ok((client, auth))
///}
///
////// Handles uploading the demo file.
///async fn upload_demo(config: &Config, file_name: &str) -> Result<Option<String>> {
///    // Ref: https://docs.rs/raze/0.4.1/raze/api/fn.b2_authorize_account.html
///    let (client, auth) = b2_client_and_auth(config).await.unwrap();
///
///    let upload_auth = b2_get_upload_url(&client, &auth, config.backblaze.bucket.clone())
///        .await
///        .unwrap();
///    let file = tokio::fs::File::open(format!("./demos/{}", file_name))
///        .await
///        .unwrap();
///    let metadata = file.metadata().await.unwrap();
///    let size = metadata.len();
///    let modf = metadata
///        .modified()
///        .unwrap()
///        .duration_since(std::time::UNIX_EPOCH)
///        .unwrap()
///        .as_secs()
///        * 1000;
///
///    let param = FileParameters {
///        file_path: file_name,
///        file_size: size,
///        content_type: None,
///        content_sha1: Sha1Variant::HexAtEnd,
///        last_modified_millis: modf,
///    };
///
///    let stream = reader_to_stream(file);
///    let stream = BytesStreamHashAtEnd::wrap(stream);
///    let stream = BytesStreamThrottled::wrap(stream, 500000000);
///
///    let body = reqwest::Body::wrap_stream(stream);
///    let resp1 = b2_upload_file(&client, &upload_auth, body, param)
///        .await
///        .unwrap();
///    Ok(resp1.file_id)
///}
///
////// Takes in either a demo_id or a changelog_id, and returns a changelog entry and a demno_id.
//////
////// We return a demo_id because there is a chance that there are multiple demos uploaded for the same changelog entry,
////// and we might want to delete an older demo.
///async fn get_changelog_and_demo_id(query: DemoOptions, pool: &PgPool) -> Result<(Changelog, i64)> {
///    if let Some(cl_id) = query.cl_id {
///        // Find the demo_id currently associated with the changelog entry.
///        let changelog = Changelog::get_changelog(pool, cl_id).await?;
///        if let Some(cl) = changelog {
///            match cl.demo_id {
///                Some(demo_id) => Ok((cl, demo_id)),
///                None => bail!("Changelog does not have a demo_id"),
///            }
///        } else {
///            bail!("No changelog entry found to match changelog_id")
///        }
///    } else if let Some(d_id) = query.demo_id {
///        let d = Demos::get_demo(pool, d_id).await?;
///        if let Some(d) = d {
///            let changelog = Changelog::get_changelog(pool, d.cl_id).await?;
///            if let Some(cl) = changelog {
///                Ok((cl, d_id))
///            } else {
///                bail!("Changelog entry referenced by demo does not exist")
///            }
///        } else {
///            bail!("No demo found")
///        }
///    } else {
///        bail!("Neither a demo or changelog ID was supplied")
///    }
///}
///
////// Deletes the demo from backblaze.
///async fn delete_demo_file(
///    pool: &PgPool,
///    config: &Config,
///    cl: Changelog,
///    demo_id: i64,
///) -> Result<()> {
///    let (client, auth) = b2_client_and_auth(config).await.unwrap();
///    let d = Demos::get_demo(pool, demo_id).await.unwrap().unwrap();
///    let file_name = generate_file_name(pool, cl).await?;
///    match b2_delete_file_version(&client, &auth, file_name, d.file_id).await {
///        Ok(_) => Ok(()),
///        Err(e) => {
///            eprintln!("Failed to delete file -> {:#?}", e);
///            bail!("Failed to delete file from BackBlaze");
///        }
///    }
///}
///
////// Once the file has been removed, delete the demo entry.
///async fn delete_demo_db(pool: &PgPool, demo_id: i64) -> std::result::Result<Demos, sqlx::Error> {
///    // Delete references to the demo_id in the changelog table.
///    Changelog::delete_references_to_demo(pool, demo_id).await?;
///    // Delete the demo entry.
///    Demos::delete_demo(pool, demo_id).await
///}
///
////// Create file_name
///async fn generate_file_name(pool: &PgPool, cl: Changelog) -> Result<String> {
///    let mut map_name = Maps::get_map_name(pool, cl.map_id).await?.unwrap();
///    map_name.retain(|c| !c.is_whitespace());
///    Ok(format!("{}_{}_{}", map_name, cl.score, cl.profile_number))
///}
