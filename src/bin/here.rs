use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        usage(&args[0]);
        return;
    }

    let port: u16 = env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(9001);
    let admin_port = port + 1;
    let admin_token = env::var("ADMIN_TOKEN").unwrap_or_else(|_| {
        eprintln!("Error: ADMIN_TOKEN not set");
        std::process::exit(1);
    });
    let base = format!("http://127.0.0.1:{admin_port}");

    match args[1].as_str() {
        "add-user" => {
            if args.len() < 3 {
                eprintln!("Usage: {} add-user <name>", args[0]);
                std::process::exit(1);
            }
            cmd_add_user(&base, &admin_token, &args[2]);
        }
        "list-users" => cmd_list_users(&base, &admin_token),
        "delete-user" => {
            if args.len() < 3 {
                eprintln!("Usage: {} delete-user <id>", args[0]);
                std::process::exit(1);
            }
            cmd_delete_user(&base, &admin_token, &args[2]);
        }
        "rotate-token" => {
            if args.len() < 3 {
                eprintln!("Usage: {} rotate-token <id>", args[0]);
                std::process::exit(1);
            }
            cmd_rotate_token(&base, &admin_token, &args[2]);
        }
        _ => usage(&args[0]),
    }
}

fn post_json(
    url: &str,
    token: &str,
    body: Option<&serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let req = ureq::post(url).header("X-Admin-Token", token);
    let resp = match body {
        Some(b) => req.send_json(b),
        None => req.send_empty(),
    }
    .map_err(|e| format!("{e}"))?;
    Ok(read_body(resp))
}

fn get_json(url: &str, token: &str) -> Result<serde_json::Value, String> {
    let resp = ureq::get(url)
        .header("X-Admin-Token", token)
        .call()
        .map_err(|e| format!("{e}"))?;
    Ok(read_body(resp))
}

fn delete_json(url: &str, token: &str) -> Result<serde_json::Value, String> {
    let resp = ureq::delete(url)
        .header("X-Admin-Token", token)
        .call()
        .map_err(|e| format!("{e}"))?;
    Ok(read_body(resp))
}

fn read_body(resp: ureq::http::Response<ureq::Body>) -> serde_json::Value {
    resp.into_body()
        .read_to_string()
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::Value::Null)
}

fn cmd_add_user(base: &str, token: &str, name: &str) {
    match post_json(
        &format!("{base}/users"),
        token,
        Some(&serde_json::json!({"name": name})),
    ) {
        Ok(body) => {
            println!("User created:");
            println!("  ID:    {}", body["id"].as_str().unwrap_or("-"));
            println!("  Name:  {}", body["name"].as_str().unwrap_or("-"));
            println!("  Token: {}", body["api_token"].as_str().unwrap_or("-"));
        }
        Err(e) => eprintln!("Error: {e}"),
    }
}

fn cmd_list_users(base: &str, token: &str) {
    match get_json(&format!("{base}/users"), token) {
        Ok(body) => {
            if let Some(users) = body.as_array() {
                if users.is_empty() {
                    println!("No users found.");
                } else {
                    for u in users {
                        println!(
                            "{}  {}  {}",
                            u["id"].as_str().unwrap_or("-"),
                            u["name"].as_str().unwrap_or("-"),
                            u["api_token"].as_str().unwrap_or("-")
                        );
                    }
                }
            }
        }
        Err(e) => eprintln!("Error: {e}"),
    }
}

fn cmd_delete_user(base: &str, token: &str, id: &str) {
    match delete_json(&format!("{base}/users/{id}"), token) {
        Ok(_) => println!("User {id} deleted."),
        Err(e) => eprintln!("Error: {e}"),
    }
}

fn cmd_rotate_token(base: &str, token: &str, id: &str) {
    match post_json(&format!("{base}/users/{id}/rotate"), token, None) {
        Ok(body) => println!(
            "New token for {id}: {}",
            body["token"].as_str().unwrap_or("-")
        ),
        Err(e) => eprintln!("Error: {e}"),
    }
}

fn usage(prog: &str) {
    eprintln!("Usage:");
    eprintln!("  {prog} add-user <name>        Create a user, returns token");
    eprintln!("  {prog} list-users              List all users");
    eprintln!("  {prog} delete-user <id>        Delete a user and their data");
    eprintln!("  {prog} rotate-token <id>       Generate a new token for a user");
    eprintln!();
    eprintln!("  ADMIN_TOKEN env var  → required");
    eprintln!("  PORT env var         → admin port = PORT+1 (default 9001 → 9002)");
}
