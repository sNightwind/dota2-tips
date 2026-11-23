mod catalog;

use catalog::{auth_matches, game_state, Catalog};
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const DEFAULT_BIND: &str = "127.0.0.1:5394";
const DEFAULT_TOKEN: &str = "dota2-tips";
const DEFAULT_OUT: &str = "captures";

struct Options {
    bind: String,
    token: String,
    out_dir: PathBuf,
    save_all: bool,
    self_test: bool,
}

fn main() -> ExitCode {
    enable_utf8_console();
    let options = match Options::from_args() {
        Ok(options) => options,
        Err(ArgError::Help) => {
            println!("{}", help_text());
            return ExitCode::SUCCESS;
        }
        Err(ArgError::Message(message)) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };

    if options.self_test {
        return match run_self_test() {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("自检失败：{message}");
                ExitCode::from(1)
            }
        };
    }

    if let Err(message) = serve(&options) {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn serve(options: &Options) -> Result<(), String> {
    let server = tiny_http::Server::http(&options.bind)
        .map_err(|error| format!("无法监听 {}：{error}", options.bind))?;
    let server = Arc::new(server);
    install_ctrlc(&server);
    let out_dir = absolute_dir(&options.out_dir)?;
    fs::create_dir_all(&out_dir).map_err(|error| format!("无法创建输出目录：{error}"))?;

    println!("dota2-tips 字段采集");
    println!("监听 http://{}", options.bind);
    println!("token {}", options.token);
    println!("输出目录 {}", out_dir.display());
    println!("等待 Dota 的 POST。Ctrl+C 结束。");
    println!("浏览器打开 http://{}/ 可以看当前字段数量。", options.bind);

    let catalog = Arc::new(Mutex::new(Catalog::default()));
    let mut accepted = 0_u64;
    let mut richest_leaves: HashMap<String, usize> = HashMap::new();

    for mut request in server.incoming_requests() {
        if request.method() == &tiny_http::Method::Get {
            let count = catalog.lock().expect("catalog lock").fields.len();
            let body = format!("已记录 {count} 个字段路径。详见输出目录里的 fields.txt。\n");
            let _ = request.respond(tiny_http::Response::from_string(body));
            continue;
        }

        if request.method() != &tiny_http::Method::Post {
            let _ = request.respond(text_response(405, "only POST"));
            continue;
        }

        let mut bytes = Vec::new();
        if let Err(error) = request.as_reader().read_to_end(&mut bytes) {
            eprintln!("读取请求失败：{error}");
            let _ = request.respond(text_response(400, "unreadable body"));
            continue;
        }

        let value: Value = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("JSON 无法解析：{error}");
                let _ = request.respond(text_response(400, "invalid json"));
                continue;
            }
        };

        if !auth_matches(&value, &options.token) {
            eprintln!("已拒绝一份载荷：auth 中没有匹配的 token");
            let _ = request.respond(text_response(401, "bad token"));
            continue;
        }

        let _ = request.respond(text_response(200, "ok"));

        accepted += 1;
        let state = game_state(&value).unwrap_or("unknown").to_string();
        let new_paths = {
            let mut catalog = catalog.lock().expect("catalog lock");
            let new_paths = catalog.observe(&value);
            if let Err(error) = catalog.write_reports(&out_dir, &value) {
                eprintln!("写入字段报告失败：{error}");
            }
            new_paths
        };

        let leaf_count = count_leaves(&value);
        if leaf_count > richest_leaves.get(&state).copied().unwrap_or(0) {
            richest_leaves.insert(state.clone(), leaf_count);
            match catalog.lock().expect("catalog lock").save_sample(&out_dir, &state, &value) {
                Ok(path) => println!("已更新样本 {}", path.display()),
                Err(error) => eprintln!("写入样本失败：{error}"),
            }
        }

        if options.save_all {
            let stamp = format!("{accepted:06}");
            let path = out_dir.join("all").join(format!("{stamp}.json"));
            if let Err(error) = write_numbered(&path, &value) {
                eprintln!("保存原始载荷失败：{error}");
            }
        }

        println!(
            "第 {accepted} 包  {marker}  字段 {}  新增 {}",
            catalog.lock().expect("catalog lock").fields.len(),
            new_paths.len(),
            marker = marker(&value),
        );
        for path in new_paths.iter().take(30) {
            println!("  + {path}");
        }
        if new_paths.len() > 30 {
            println!("  … 另有 {} 个新字段，见 fields.txt", new_paths.len() - 30);
        }
    }

    println!("采集结束。字段报告仍在 {}", out_dir.display());
    Ok(())
}

fn install_ctrlc(server: &Arc<tiny_http::Server>) {
    let server = Arc::clone(server);
    if let Err(error) = ctrlc::set_handler(move || {
        server.unblock();
    }) {
        eprintln!("未能接管 Ctrl+C：{error}");
    }
}

fn marker(value: &Value) -> String {
    let state = game_state(value).unwrap_or("-");
    let clock = value
        .pointer("/map/clock_time")
        .map(ToString::to_string)
        .unwrap_or_else(|| "-".to_string());
    let hero = value.pointer("/hero/name").and_then(Value::as_str).unwrap_or("-");
    format!("状态={state}  时钟={clock}  英雄={hero}")
}

fn count_leaves(value: &Value) -> usize {
    match value {
        Value::Array(items) => items.iter().map(count_leaves).sum::<usize>().max(1),
        Value::Object(map) => {
            if map.is_empty() {
                1
            } else {
                map.values().map(count_leaves).sum()
            }
        }
        _ => 1,
    }
}

fn write_numbered(path: &Path, value: &Value) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value).unwrap())
}

fn text_response(status: u16, body: &'static str) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    tiny_http::Response::from_data(body.as_bytes()).with_status_code(status)
}

fn absolute_dir(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let current = env::current_dir().map_err(|error| format!("无法读取当前目录：{error}"))?;
    Ok(current.join(path))
}

fn run_self_test() -> Result<(), String> {
    let bind = "127.0.0.1:5395";
    let out_dir = env::temp_dir().join("dota2-tips-self-test");
    let _ = fs::remove_dir_all(&out_dir);
    let options = Options {
        bind: bind.to_string(),
        token: DEFAULT_TOKEN.to_string(),
        out_dir,
        save_all: false,
        self_test: false,
    };
    let out_dir = options.out_dir.clone();
    thread::spawn(move || {
        if let Err(message) = serve(&options) {
            eprintln!("{message}");
        }
    });

    let body = r#"{"auth":{"token":"dota2-tips"},"provider":{"name":"Dota 2","appid":570},"map":{"name":"hero_demo","matchid":"1","game_time":30,"clock_time":-30,"daytime":true,"game_state":"DOTA_GAMERULES_STATE_PRE_GAME","paused":false},"hero":{"name":"npc_dota_hero_axe","stunned":false,"health":100},"abilities":[{"name":"axe_berserkers_call","level":1,"cooldown":0}]}"#;
    post_json(bind, body)?;

    let fields = out_dir.join("fields.txt");
    let ready = wait_for_file(&fields, "abilities[].cooldown")?;
    if !ready.contains("map.clock_time") || !ready.contains("hero.stunned") {
        return Err("字段报告里缺少预期路径".to_string());
    }
    println!("自检通过。监听、token 和字段落盘可用。");
    println!("报告：{}", fields.display());
    Ok(())
}

fn post_json(bind: &str, body: &str) -> Result<(), String> {
    let mut last_error = "连接失败".to_string();
    for _ in 0..40 {
        match TcpStream::connect(bind) {
            Ok(mut stream) => {
                let request = format!(
                    "POST / HTTP/1.1\r\nHost: {bind}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(request.as_bytes())
                    .map_err(|error| format!("发送自检请求失败：{error}"))?;
                let mut response = String::new();
                stream
                    .read_to_string(&mut response)
                    .map_err(|error| format!("读取自检响应失败：{error}"))?;
                if !response.contains("200") {
                    return Err(format!("自检响应不是 200：{response}"));
                }
                return Ok(());
            }
            Err(error) => {
                last_error = error.to_string();
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
    Err(format!("自检服务没有启动：{last_error}"))
}

fn wait_for_file(path: &Path, needle: &str) -> Result<String, String> {
    for _ in 0..40 {
        if let Ok(text) = fs::read_to_string(path) {
            if text.contains(needle) {
                return Ok(text);
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err(format!("超时，没有等到 {}", path.display()))
}

enum ArgError {
    Help,
    Message(String),
}

impl Options {
    fn from_args() -> Result<Self, ArgError> {
        let mut options = Options {
            bind: DEFAULT_BIND.to_string(),
            token: DEFAULT_TOKEN.to_string(),
            out_dir: PathBuf::from(DEFAULT_OUT),
            save_all: false,
            self_test: false,
        };
        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--save-all" => options.save_all = true,
                "--self-test" => options.self_test = true,
                "--bind" => {
                    options.bind = args
                        .next()
                        .ok_or_else(|| ArgError::Message("--bind 需要一个地址，例如 127.0.0.1:5394".to_string()))?;
                }
                "--token" => {
                    options.token = args
                        .next()
                        .ok_or_else(|| ArgError::Message("--token 需要一个字符串".to_string()))?;
                }
                "--out" => {
                    options.out_dir = PathBuf::from(
                        args.next()
                            .ok_or_else(|| ArgError::Message("--out 需要一个目录".to_string()))?,
                    );
                }
                "--help" | "-h" => return Err(ArgError::Help),
                other => {
                    return Err(ArgError::Message(format!(
                        "无法识别的参数 {other}\n{}",
                        help_text()
                    )));
                }
            }
        }
        Ok(options)
    }
}

fn help_text() -> String {
    format!(
        "用法：dota2-tips [--bind {DEFAULT_BIND}] [--token {DEFAULT_TOKEN}] [--out {DEFAULT_OUT}] [--save-all] [--self-test]"
    )
}

#[cfg(windows)]
fn enable_utf8_console() {
    unsafe extern "system" {
        fn SetConsoleOutputCP(code_page: u32) -> i32;
        fn SetConsoleCP(code_page: u32) -> i32;
    }
    unsafe {
        SetConsoleOutputCP(65001);
        SetConsoleCP(65001);
    }
}

#[cfg(not(windows))]
fn enable_utf8_console() {}
