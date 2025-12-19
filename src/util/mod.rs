pub mod message_store;
pub mod url_utils;

/// Mask credentials in a connection URL (e.g., redis/mysql/http), preserving scheme/host while redacting password.
/// Examples:
/// - "mysql://user:secret@127.0.0.1:3306/db" -> "mysql://user:****@127.0.0.1:3306/db"
/// - "redis://:p%40ss@localhost:6379/0" -> "redis://:****@localhost:6379/0"
pub fn mask_url_credentials(url: &str) -> String {
	if let Some(scheme_end) = url.find("://") {
		let (scheme_part, rest) = url.split_at(scheme_end + 3);
		if let Some(at_pos) = rest.find('@') {
			let (userinfo, remainder) = rest.split_at(at_pos);
			let masked_userinfo = mask_userinfo(userinfo);
			let remainder_no_at = remainder.strip_prefix('@').unwrap_or(remainder);
			return format!("{}{}@{}", scheme_part, masked_userinfo, remainder_no_at);
		}
	}

	url.to_string()
}

fn mask_userinfo(userinfo: &str) -> String {
	if userinfo.is_empty() {
		return String::new();
	}

	if let Some(colon_pos) = userinfo.find(':') {
		let username = &userinfo[..colon_pos];
		if username.is_empty() {
			return ":****".to_string();
		}
		format!("{}:****", username)
	} else {
		"****".to_string()
	}
}
