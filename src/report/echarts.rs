//! Renders an ECharts option object as a self-contained HTML page.
//!
//! The ECharts bundle is inlined rather than linked so the output keeps
//! working offline, after being copied elsewhere, or years later when the CDN
//! has moved on.

use serde_json::Value;

const TEMPLATE: &str = include_str!("chart.html");
const ECHARTS: &str = include_str!("../../vendor/echarts.min.js");

pub fn render_html(option: &Value, title: &str, subtitle: &str) -> Result<String, serde_json::Error> {
    // Account and commodity names are alphanumeric, so neither can close the
    // script element or the surrounding tags.
    Ok(TEMPLATE
        .replace("{{TITLE}}", title)
        .replace("{{SUBTITLE}}", subtitle)
        .replace("{{OPTION}}", &serde_json::to_string(option)?)
        .replace("{{ECHARTS}}", ECHARTS))
}
