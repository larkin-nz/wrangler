use crate::http;
use crate::settings::global_user::GlobalUser;
use crate::terminal::emoji;

use cloudflare::endpoints::account;
use cloudflare::framework::apiclient::ApiClient;
use cloudflare::framework::HttpApiClientConfig;

use prettytable::{Cell, Row, Table};

pub fn whoami(user: &GlobalUser) -> Result<(), failure::Error> {
    // If using email + API key for auth, simply prints out email from config file.
    let auth: String = match user {
        GlobalUser::GlobalKeyAuth { email, .. } => {
            format!(
                "a Global API Key, associated with the email '{}'",
                email,
            )
        }
        GlobalUser::TokenAuth { .. } => {
            format!("an API Token")
        }
    };

    println!(
        "\n{} You are logged in with {}.\n",
        emoji::WAVING,
        auth,
    );
    let table = match fetch_accounts(user) {
        Ok(table) => table,
        Err(e) => failure::bail!(http::format_error(e, None)),
    };
    println!("{}", &table);
    Ok(())
}

fn fetch_accounts(user: &GlobalUser) -> Result<String, failure::Error> {
    let client = http::cf_v4_api_client(user, HttpApiClientConfig::default())?;
    let response = client.request(&account::ListAccounts { params: None })?;

    let mut table = Table::new();
    let table_head = Row::new(vec![Cell::new("Account Name"), Cell::new("Account ID")]);
    table.add_row(table_head);

    match user {
        GlobalUser::TokenAuth { .. } => {
            if response.result.is_empty() {
                println!("Your token is missing the 'Account Settings: Read' permission.\n\nPlease generate and auth with a new token that has these perms to be able to list your accounts.\n");
            }
        }
        _ => (),
    }

    for account in response.result {
        let row = Row::new(vec![Cell::new(&account.name), Cell::new(&account.id)]);
        table.add_row(row);
    }
    Ok(table.to_string())
}
