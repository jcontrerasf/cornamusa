// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `cornamusa mcp`: stdio bridge for Claude Code / Claude Desktop (doesn't start the UI).
    if std::env::args().nth(1).as_deref() == Some("mcp") {
        std::process::exit(cornamusa_lib::mcp_bridge());
    }
    cornamusa_lib::run()
}
