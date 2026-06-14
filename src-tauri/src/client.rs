use crate::auth::CookieEntry;
use crate::models::{
    DailyCostEntry, ModelCallCount, ModelCallStats, UsageInfo, UsagePeriod, UsageRecord,
};
use chrono::Datelike;
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderValue, COOKIE};
use reqwest::Client;
use std::collections::HashMap;

const X_SERVER_ID_COSTS: &str = "15702f3a12ff8bff357f8c2aa154a17e65b746d5f6b96adc9002c86ee0c15205";
const X_SERVER_ID_USAGE: &str = "bfd684bfc2e4eed05cd0b518f5e4eafd3f3376e3938abb9e536e7c03df831e5c";

pub struct OpenCodeClient {
    client: Client,
    base_url: String,
}

/// URL 编码辅助函数
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

impl OpenCodeClient {
    pub fn new() -> Result<Self, String> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            client,
            base_url: "https://opencode.ai".into(),
        })
    }

    /// 寻找字符串中双引号包裹的文本结束位置（跳过转义的双引号）
    fn find_string_end(bytes: &[u8], mut i: usize) -> usize {
        i += 1;
        while i < bytes.len() {
            if bytes[i] == b'\\' && i + 1 < bytes.len() {
                i += 2;
            } else if bytes[i] == b'"' {
                return i + 1;
            } else {
                i += 1;
            }
        }
        i
    }

    /// 构造包含 Cookie 的请求头
    fn headers_with_cookies(cookies: &[CookieEntry]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if !cookies.is_empty() {
            let cookie_str = cookies
                .iter()
                .map(|c| format!("{}={}", c.name, c.value))
                .collect::<Vec<_>>()
                .join("; ");
            if let Ok(val) = HeaderValue::from_str(&cookie_str) {
                headers.insert(COOKIE, val);
            }
        }
        headers
    }

    /// 统一分发 SolidStart 服务端函数请求
    async fn call_server_fn(
        &self,
        cookies: &[CookieEntry],
        x_server_id: &'static str,
        x_server_instance: &'static str,
        args: &serde_json::Value,
    ) -> Result<String, String> {
        let url = format!("{}/_server", self.base_url);
        let mut headers = Self::headers_with_cookies(cookies);
        headers.insert("x-server-id", HeaderValue::from_static(x_server_id));
        headers.insert(
            "x-server-instance",
            HeaderValue::from_static(x_server_instance),
        );

        let body_str = serde_json::to_string(args).map_err(|e| e.to_string())?;

        let req_builder = if x_server_instance == "server-fn:0" {
            headers.insert("Content-Type", HeaderValue::from_static("application/json"));
            self.client.post(&url).headers(headers).body(body_str)
        } else {
            let full_url = format!("{}?id={}&args={}", url, x_server_id, urlencode(&body_str));
            self.client.get(&full_url).headers(headers)
        };

        let resp = req_builder
            .send()
            .await
            .map_err(|e| format!("_server network error: {}", e))?;
        resp.text().await.map_err(|e| format!("Read error: {}", e))
    }

    pub async fn fetch_usage(
        &self,
        cookies: &[CookieEntry],
        workspace_id: &str,
    ) -> Result<UsageInfo, String> {
        let url = format!("{}/workspace/{}/go", self.base_url, workspace_id);
        let resp = self
            .client
            .get(&url)
            .headers(Self::headers_with_cookies(cookies))
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        match resp.status() {
            reqwest::StatusCode::OK => {
                let html = resp
                    .text()
                    .await
                    .map_err(|e| format!("Read error: {}", e))?;
                Self::parse_usage_from_html(&html)
            }
            reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
                Err("AUTH_EXPIRED".into())
            }
            reqwest::StatusCode::FOUND | reqwest::StatusCode::MOVED_PERMANENTLY => {
                Err("REDIRECT_TO_LOGIN".into())
            }
            other => Err(format!("HTTP error: {}", other)),
        }
    }

    fn parse_usage_from_html(html: &str) -> Result<UsageInfo, String> {
        let parse_period = |pattern: &str| -> Result<UsagePeriod, String> {
            let re = Regex::new(pattern).map_err(|e| e.to_string())?;
            let caps = re
                .captures(html)
                .ok_or_else(|| format!("Failed to match: {}", pattern))?;
            Ok(UsagePeriod {
                status: caps[1].to_string(),
                usage_percent: caps[3].parse().unwrap_or(0),
                reset_in_sec: caps[2].parse().unwrap_or(0),
            })
        };

        Ok(UsageInfo {
            rolling: parse_period(
                r#"rollingUsage:\$R\[\d+\]=\{status:"(\w+)",resetInSec:(\d+),usagePercent:(\d+)\}"#,
            )?,
            weekly: parse_period(
                r#"weeklyUsage:\$R\[\d+\]=\{status:"(\w+)",resetInSec:(\d+),usagePercent:(\d+)\}"#,
            )?,
            monthly: parse_period(
                r#"monthlyUsage:\$R\[\d+\]=\{status:"(\w+)",resetInSec:(\d+),usagePercent:(\d+)\}"#,
            )?,
        })
    }

    pub async fn fetch_monthly_costs(
        &self,
        cookies: &[CookieEntry],
        workspace_id: &str,
    ) -> Result<Vec<DailyCostEntry>, String> {
        let now = chrono::Utc::now();
        let offset = chrono::Local::now().offset().local_minus_utc();
        let tz_str = format!(
            "{}{:02}:{:02}",
            if offset >= 0 { "+" } else { "-" },
            offset.abs() / 3600,
            (offset.abs() % 3600) / 60
        );

        let args = serde_json::json!({
            "t": {"t": 9, "i": 0, "l": 4,
                "a": [
                    {"t": 1, "s": workspace_id},
                    {"t": 0, "s": now.year()},
                    {"t": 0, "s": now.month0()},
                    {"t": 1, "s": tz_str}
                ],
                "o": 0
            },
            "f": 31,
            "m": []
        });

        let body_text = self
            .call_server_fn(cookies, X_SERVER_ID_COSTS, "server-fn:0", &args)
            .await?;
        Self::parse_server_response(&body_text)
    }

    pub(crate) fn parse_server_response(text: &str) -> Result<Vec<DailyCostEntry>, String> {
        let usage_start = text
            .find("usage:$R[")
            .ok_or("Missing usage key in _server response")?;
        let after_key = &text[usage_start + "usage:$R".len()..];
        let digits_end = after_key
            .find('=')
            .ok_or("Malformed usage: $R entry in _server response")?;

        let abs_start = usage_start + "usage:$R".len() + digits_end + 1;
        let bytes = text.as_bytes();

        if abs_start >= bytes.len() || bytes[abs_start] != b'[' {
            return Err("Expected '[' after usage:$R[N]= in _server response".into());
        }

        let mut depth = 1;
        let mut abs_end = 0;
        for (idx, &b) in bytes.iter().enumerate().skip(abs_start + 1) {
            match b {
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        abs_end = idx + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        if abs_end <= abs_start {
            return Err("Unbalanced brackets in usage array".into());
        }

        let array_str = &text[abs_start..abs_end];
        let resolved = Self::resolve_r_references(array_str);
        let json_str = Self::js_object_array_to_json(&resolved)
            .map_err(|e| format!("_server parse error: {}", e))?;

        serde_json::from_str(&json_str).map_err(|e| {
            format!(
                "_server parse error: {} — json head: {}",
                e,
                &json_str[..200.min(json_str.len())]
            )
        })
    }

    fn resolve_r_references(input: &str) -> String {
        let mut defs = Vec::new();
        Self::resolve_r_references_impl(input, &mut defs)
    }

    fn resolve_r_references_impl(input: &str, defs: &mut Vec<String>) -> String {
        let bytes = input.as_bytes();
        let mut out = String::with_capacity(input.len());
        let mut i = 0;

        while i < bytes.len() {
            if bytes[i..].starts_with(b"$R[") {
                let mut j = i + 3;
                let n_start = j;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b']' {
                    let n: usize = input[n_start..j].parse().unwrap_or(0);
                    j += 1; // 越过 ']'

                    while defs.len() <= n {
                        defs.push(String::new());
                    }

                    if j < bytes.len() && bytes[j] == b'=' {
                        let (val_str, val_end) = Self::parse_r_value(input, j + 1);
                        let resolved_val = Self::resolve_r_references_impl(&val_str, defs);
                        defs[n] = resolved_val.clone();
                        out.push_str(&resolved_val);
                        i = val_end;
                        continue;
                    } else {
                        out.push_str(if n < defs.len() && !defs[n].is_empty() {
                            &defs[n]
                        } else {
                            "null"
                        });
                        i = j;
                        continue;
                    }
                }
            }
            out.push(bytes[i] as char);
            i += 1;
        }
        out
    }

    fn parse_r_value(input: &str, start: usize) -> (String, usize) {
        let bytes = input.as_bytes();
        let mut i = start;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            return (String::new(), i);
        }

        match bytes[i] {
            open @ (b'{' | b'[') => {
                let close = if open == b'{' { b'}' } else { b']' };
                let val_start = i;
                let mut depth = 1;
                i += 1;
                while i < bytes.len() && depth > 0 {
                    if bytes[i] == b'"' {
                        i = Self::find_string_end(bytes, i);
                    } else {
                        if bytes[i] == open {
                            depth += 1;
                        } else if bytes[i] == close {
                            depth -= 1;
                        }
                        i += 1;
                    }
                }
                (input[val_start..i].to_string(), i)
            }
            b'"' => {
                let end = Self::find_string_end(bytes, i);
                (input[i..end].to_string(), end)
            }
            b't' | b'f' | b'n' => {
                let start_val = i;
                while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let id = &input[start_val..i];
                if id == "new" {
                    let mut k = i;
                    while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                        k += 1;
                    }
                    if k < bytes.len() && (bytes[k].is_ascii_alphabetic() || bytes[k] == b'_') {
                        let ctor_start = k;
                        while k < bytes.len()
                            && (bytes[k].is_ascii_alphanumeric() || bytes[k] == b'_')
                        {
                            k += 1;
                        }
                        let _ = &input[ctor_start..k];
                    }
                    if k < bytes.len() && bytes[k] == b'(' {
                        let paren_open = k;
                        let mut depth = 1;
                        k += 1;
                        while k < bytes.len() && depth > 0 {
                            if bytes[k] == b'(' {
                                depth += 1;
                            } else if bytes[k] == b')' {
                                depth -= 1;
                            }
                            k += 1;
                        }
                        (input[paren_open + 1..k - 1].to_string(), k)
                    } else {
                        (String::new(), k)
                    }
                } else {
                    (id.to_string(), i)
                }
            }
            b'!' => {
                let start_val = i;
                if i + 1 < bytes.len() && (bytes[i + 1] == b'1' || bytes[i + 1] == b'0') {
                    i += 2;
                }
                (input[start_val..i].to_string(), i)
            }
            b'-' | b'0'..=b'9' => {
                let start_val = i;
                if bytes[i] == b'-' {
                    i += 1;
                }
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                    i += 1;
                }
                (input[start_val..i].to_string(), i)
            }
            _ => (String::new(), i + 1),
        }
    }

    fn js_object_array_to_json(input: &str) -> Result<String, String> {
        let bytes = input.as_bytes();
        let mut out = String::with_capacity(input.len() + 64);
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if b == b'"' {
                let end = Self::find_string_end(bytes, i);
                out.push_str(&input[i..end]);
                i = end;
                continue;
            }

            if bytes[i..].starts_with(b"new Date(") {
                let paren_start = i + 9;
                if paren_start < bytes.len() && bytes[paren_start] == b'"' {
                    let k = Self::find_string_end(bytes, paren_start);
                    let end = if k < bytes.len() && bytes[k] == b')' {
                        k + 1
                    } else {
                        k
                    };
                    out.push_str(&input[paren_start..k]);
                    i = end;
                    continue;
                }
            }

            if b.is_ascii_alphabetic() || b == b'_' {
                let id_start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let id = &input[id_start..i];

                let mut j = i;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b':' {
                    out.push('"');
                    out.push_str(id);
                    out.push_str("\":");
                    i = j + 1;
                    continue;
                } else {
                    out.push_str(id);
                    continue;
                }
            }

            if b == b'!' && i + 1 < bytes.len() && (bytes[i + 1] == b'1' || bytes[i + 1] == b'0') {
                out.push_str(if bytes[i + 1] == b'0' {
                    "true"
                } else {
                    "false"
                });
                i += 2;
                continue;
            }

            out.push(b as char);
            i += 1;
        }
        Ok(out)
    }

    pub async fn fetch_usage_page(
        &self,
        cookies: &[CookieEntry],
        workspace_id: &str,
        page: u32,
    ) -> Result<Vec<UsageRecord>, String> {
        let args = serde_json::json!({
            "t": {
                "t": 9, "i": 0, "l": 2,
                "a": [
                    {"t": 1, "s": workspace_id},
                    {"t": 0, "s": page}
                ],
                "o": 0
            },
            "f": 31,
            "m": []
        });

        let body_text = self
            .call_server_fn(cookies, X_SERVER_ID_USAGE, "server-fn:1", &args)
            .await?;
        Self::parse_usage_list_response(&body_text)
    }

    pub(crate) fn parse_usage_list_response(text: &str) -> Result<Vec<UsageRecord>, String> {
        let bytes = text.as_bytes();
        let mut best_start = None;
        let mut best_end = None;
        let mut best_len = 0;

        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'"' {
                i = Self::find_string_end(bytes, i);
            } else if bytes[i] == b'[' {
                let start = i;
                let mut depth = 1;
                i += 1;
                while i < bytes.len() && depth > 0 {
                    if bytes[i] == b'"' {
                        i = Self::find_string_end(bytes, i);
                    } else if bytes[i] == b'[' {
                        depth += 1;
                        i += 1;
                    } else if bytes[i] == b']' {
                        depth -= 1;
                        i += 1;
                    } else {
                        i += 1;
                    }
                }
                if depth == 0 {
                    let len = i - start;
                    if len > best_len {
                        best_len = len;
                        best_start = Some(start);
                        best_end = Some(i);
                    }
                }
            } else {
                i += 1;
            }
        }

        let (s, e) = match (best_start, best_end) {
            (Some(s), Some(e)) if e > s => (s, e),
            _ => return Ok(Vec::new()),
        };
        Self::parse_record_array(&text[s..e])
    }

    fn parse_record_array(array_content: &str) -> Result<Vec<UsageRecord>, String> {
        let resolved = Self::resolve_r_references(array_content);
        let json_str = Self::js_object_array_to_json(&resolved)
            .map_err(|e| format!("usage.list parse error: {}", e))?;
        serde_json::from_str::<Vec<UsageRecord>>(&json_str).map_err(|e| {
            format!(
                "usage.list JSON parse error: {} — json head: {}",
                e,
                &json_str[..200.min(json_str.len())]
            )
        })
    }

    #[allow(dead_code)]
    fn parse_single_usage_record(js_obj: &str) -> Option<UsageRecord> {
        let resolved = Self::resolve_r_references(js_obj);
        let json_str = Self::js_object_array_to_json(&resolved).ok()?;
        serde_json::from_str::<UsageRecord>(&json_str).ok()
    }

    pub async fn fetch_all_model_calls(
        &self,
        cookies: &[CookieEntry],
        workspace_id: &str,
        max_pages: u32,
    ) -> Result<(Vec<UsageRecord>, ModelCallStats), String> {
        let mut all_records = Vec::new();
        for page in 0..max_pages {
            let page_records = self.fetch_usage_page(cookies, workspace_id, page).await?;
            if page_records.is_empty() {
                break;
            }
            let fetched = page_records.len();
            all_records.extend(page_records);
            if fetched < 50 {
                break;
            }
        }
        let stats = Self::agg_stats_from_records(&all_records);
        Ok((all_records, stats))
    }

    pub(crate) fn agg_stats_from_records(records: &[UsageRecord]) -> ModelCallStats {
        let mut model_map = HashMap::new();
        for r in records {
            *model_map.entry(r.model.clone()).or_insert(0) += 1;
        }
        let mut models: Vec<(String, u64)> = model_map.into_iter().collect();
        models.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let total_calls = records.len() as u64;
        let model_counts = models
            .into_iter()
            .map(|(name, calls)| ModelCallCount {
                name,
                calls,
                percentage: if total_calls > 0 {
                    (calls as f64 / total_calls as f64) * 100.0
                } else {
                    0.0
                },
            })
            .collect();
        ModelCallStats {
            models: model_counts,
            total_calls,
        }
    }

    pub async fn fetch_model_calls(
        &self,
        cookies: &[CookieEntry],
        workspace_id: &str,
    ) -> Result<ModelCallStats, String> {
        let (_, stats) = self.fetch_all_model_calls(cookies, workspace_id, 5).await?;
        Ok(stats)
    }

    pub fn parse_initial_usage_records_from_html(html: &str) -> Vec<UsageRecord> {
        let marker = "$R[26]=[";
        let Some(start) = html.find(marker).map(|idx| idx + marker.len()) else {
            return vec![];
        };

        let mut depth = 1;
        let mut end = start;
        for (idx, &b) in html.as_bytes().iter().enumerate().skip(start) {
            match b {
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        end = idx;
                        break;
                    }
                }
                _ => {}
            }
        }
        Self::parse_record_array(&html[start..end]).unwrap_or_default()
    }
}
