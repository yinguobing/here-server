use std::env;

use here_server::db;

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        usage(&args[0]);
        return;
    }

    let db_path = env::var("DATA_DIR").unwrap_or_else(|_| "/var/lib/here-server".into());
    let db = db::init(&db_path).await.unwrap_or_else(|e| {
        eprintln!("Failed to open database at {db_path}: {e}");
        std::process::exit(1);
    });

    match args[1].as_str() {
        "add-user" => {
            if args.len() < 3 {
                eprintln!("Usage: {} add-user <name>", args[0]);
                std::process::exit(1);
            }
            match db::create_user(&db, &args[2]).await {
                Ok(user) => {
                    println!("User created:");
                    println!("  ID:    {}", user.id_str());
                    println!("  Name:  {}", user.name);
                    println!("  Token: {}", user.api_token);
                }
                Err(e) => eprintln!("Error: {e}"),
            }
        }
        "list-users" => match db::list_users(&db).await {
            Ok(users) => {
                if users.is_empty() {
                    println!("No users found.");
                } else {
                    for u in users {
                        println!("{}  {}  {}", u.id_str(), u.name, u.api_token);
                    }
                }
            }
            Err(e) => eprintln!("Error: {e}"),
        },
        "delete-user" => {
            if args.len() < 3 {
                eprintln!("Usage: {} delete-user <id>", args[0]);
                std::process::exit(1);
            }
            match db::delete_user(&db, &args[2]).await {
                Ok(()) => println!("User {} deleted.", args[2]),
                Err(e) => eprintln!("Error: {e}"),
            }
        }
        "rotate-token" => {
            if args.len() < 3 {
                eprintln!("Usage: {} rotate-token <id>", args[0]);
                std::process::exit(1);
            }
            match db::rotate_user_token(&db, &args[2]).await {
                Ok(token) => println!("New token for {}: {}", args[2], token),
                Err(e) => eprintln!("Error: {e}"),
            }
        }
        _ => usage(&args[0]),
    }
}

fn usage(prog: &str) {
    eprintln!("Usage:");
    eprintln!("  {prog} add-user <name>        Create a user, returns token");
    eprintln!("  {prog} list-users              List all users");
    eprintln!("  {prog} delete-user <id>        Delete a user and their data");
    eprintln!("  {prog} rotate-token <id>       Generate a new token for a user");
    eprintln!();
    eprintln!("  Database path: DATA_DIR env var (default: /var/lib/here-server)");
}
