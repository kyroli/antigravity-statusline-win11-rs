// TUI layout engine: widget builders for info/metric rows and final output rendering.

use std::time::{SystemTime, UNIX_EPOCH};
use crate::types::{UserConfig, InputJson, CacheData};
use crate::theme::{self, Theme, RESET, BOLD};
use crate::widget::{Widget, render_progress_bar};
use crate::path::{get_human_format, get_short_model_name, get_shorten_path, parse_rfc3339_to_unix};

fn get_model_quota_string(theme: &Theme, cache: &CacheData, current_model: &str, hide_time: bool) -> String {
    let p = theme.palette;

    if cache.needs_login == Some(true) {
        let icon = theme::get_icon("quota");
        return format!("{}{}{}{}[LOGIN]{}", p.label, icon, p.warning, BOLD, RESET);
    }

    let clean_name = |n: &str| n.to_lowercase().replace(|c: char| !c.is_alphanumeric(), "");
    let target_clean = clean_name(current_model);

    let matched = cache
        .quota
        .iter()
        .find(|item| clean_name(&item.display_name) == target_clean || clean_name(&item.id) == target_clean)
        .or_else(|| {
            cache.quota.iter().find(|item| {
                target_clean.contains(&clean_name(&item.display_name))
                    || clean_name(&item.display_name).contains(&target_clean)
            })
        });

    if let Some(item) = matched {
        let pct = (item.remaining_fraction * 100.0).floor() as i64;
        let mut time_str = String::new();

        if let Some(ref r_time) = item.reset_time {
            if !hide_time {
                if let Some(parsed_time) = parse_rfc3339_to_unix(r_time) {
                    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                    if parsed_time > now {
                        let diff_mins = (parsed_time as u64 - now) / 60;
                        let diff_hours = diff_mins / 60;
                        let diff_days = diff_hours / 24;
                        let raw_time = if diff_days >= 1 {
                            format!("~{}d{}h", diff_days, diff_hours % 24)
                        } else if diff_hours >= 1 {
                            format!("~{}h{}m", diff_hours, diff_mins % 60)
                        } else {
                            format!("~{}m", diff_mins)
                        };
                        time_str = format!(" {}({}){}", p.label, raw_time, RESET);
                    }
                }
            }
        }

        let (color, active_color) = if pct <= 20 {
            (format!("{}{}", p.critical, BOLD), p.critical)
        } else if pct <= 50 {
            (p.warning.to_string(), p.warning)
        } else {
            (p.success.to_string(), p.success)
        };

        let icon = theme::get_icon("quota");
        if !hide_time {
            let bar = render_progress_bar(item.remaining_fraction * 100.0, 5, active_color, true);
            format!("{}{}{}{}", p.label, icon, bar, time_str)
        } else {
            format!("{}{}{}{}%{}{}", p.label, icon, color, pct, RESET, time_str)
        }
    } else {
        String::new()
    }
}

fn get_info_widgets(theme: &Theme, json: &InputJson, cache: &CacheData, step: usize, cols: usize) -> Vec<Widget> {
    let mut list = Vec::new();
    let p = theme.palette;

    // State indicator
    let is_pending = json.tool_confirmation_pending.unwrap_or(false);
    let state_text = if is_pending {
        format!("{}{}[WAITING APPROVAL]{}", p.critical, BOLD, RESET)
    } else {
        let state = json.agent_state.as_deref().unwrap_or("idle");
        if state == "tool_use" {
            if let Some(ref agent) = json.agent {
                if let Some(ref status) = agent.status {
                    if !status.is_empty() && status != "idle" {
                        let mut short_status = status.to_uppercase();
                        let char_count = short_status.chars().count();
                        if char_count > 16 {
                            short_status = format!("{}..", short_status.chars().take(14).collect::<String>());
                        }
                        format!("{}{}[{}]{}", p.accent, BOLD, short_status, RESET)
                    } else {
                        theme.states.tool_use.to_string()
                    }
                } else {
                    theme.states.tool_use.to_string()
                }
            } else {
                theme.states.tool_use.to_string()
            }
        } else {
            match state {
                "idle" => theme.states.ready.to_string(),
                "thinking" => theme.states.thinking.to_string(),
                "working" => theme.states.working.to_string(),
                "tool_use" => theme.states.tool_use.to_string(),
                _ => theme.states.default_state.to_string(),
            }
        }
    };
    list.push(Widget::new(state_text));

    // Agent mode
    if let Some(ref agent) = json.agent {
        if let Some(ref name) = agent.name {
            let name_lower = name.to_lowercase();
            let special_mode = if name_lower.contains("grill") {
                Some(format!("{}{}[GRILLME]{}", p.critical, BOLD, RESET))
            } else if name_lower.contains("plan") {
                Some(format!("{}{}[PLAN]{}", p.accent, BOLD, RESET))
            } else if name_lower.contains("goal") {
                Some(format!("{}{}[GOAL]{}", p.success, BOLD, RESET))
            } else if name_lower != "default" && name_lower != "main" && !name_lower.is_empty() {
                Some(format!("{}{}[{}]{}", p.accent, BOLD, name.to_uppercase(), RESET))
            } else {
                None
            };
            if let Some(mode_text) = special_mode {
                list.push(Widget::new(mode_text));
            }
        }

        if let Some(ref status) = agent.status {
            if !status.is_empty() && status != "idle" {
                let state = json.agent_state.as_deref().unwrap_or("idle");
                let is_pending = json.tool_confirmation_pending.unwrap_or(false);
                if state != "tool_use" && !is_pending {
                    list.push(Widget::new(format!("{}{}{}", p.accent, status, RESET)));
                }
            }
        }
    }

    // Subagents
    let subs = json.subagents.as_ref().map(|s| s.len()).unwrap_or(0);
    if subs > 0 {
        list.push(Widget::new(format!("{}[+{} SUBAGENTS]{}", p.accent, subs, RESET)));
    }

    // Pending input
    let p_input = json.pending_input_count.unwrap_or(0);
    if p_input > 0 {
        list.push(Widget::new(format!("{}> {}{}", p.warning, p_input, RESET)));
    }

    // Model + Quota
    let raw_model = json.model.as_ref().and_then(|m| m.display_name.as_deref()).unwrap_or("");
    if !raw_model.is_empty() {
        let q_info = get_model_quota_string(theme, cache, raw_model, step >= 6 || cols < 80);
        let model_part = if step >= 4 { get_short_model_name(raw_model) } else { raw_model.to_string() };

        let text = if !q_info.is_empty() {
            format!("{}{}{} {}|{} {}", p.accent, model_part, RESET, p.border, p.border, q_info)
        } else {
            format!("{}{}{}", p.accent, model_part, RESET)
        };
        if !text.is_empty() {
            list.push(Widget::new(text));
        }
    }

    // Working directory
    let raw_cwd = json.workspace.as_ref().and_then(|w| w.current_dir.as_deref())
        .or_else(|| json.cwd.as_deref()).unwrap_or("");
    if !raw_cwd.is_empty() && step < 5 {
        let path_text = if step >= 3 {
            raw_cwd.replace('\\', "/").split('/').last().unwrap_or(raw_cwd).to_string()
        } else {
            get_shorten_path(raw_cwd)
        };
        let icon = theme::get_icon("path");
        list.push(Widget::new(format!("{}{}{}{}", p.label, icon, path_text, RESET)));
    }

    // VCS branch
    if let Some(ref vcs) = cache.vcs {
        if vcs.cwd == raw_cwd && !vcs.branch.is_empty() && step < 6 {
            let mut branch_text = vcs.branch.clone();
            let char_count = branch_text.chars().count();
            if step >= 4 {
                if char_count > 10 {
                    branch_text = format!("{}..", branch_text.chars().take(8).collect::<String>());
                }
            } else if char_count > 15 {
                branch_text = format!("{}..", branch_text.chars().take(12).collect::<String>());
            }
            let icon = theme::get_icon("vcs");
            let label = format!("{}{}", icon, branch_text);

            let mut git_extra = String::new();
            if vcs.dirty {
                if vcs.modified > 0 && step < 4 {
                    git_extra.push_str(&format!("*{}", vcs.modified));
                } else {
                    git_extra.push('*');
                }
            }
            if step < 4 {
                if vcs.ahead > 0 && vcs.behind > 0 {
                    git_extra.push_str(&format!(" \u{2191}{}\u{2193}{}", vcs.ahead, vcs.behind));
                } else if vcs.ahead > 0 {
                    git_extra.push_str(&format!(" \u{2191}{}", vcs.ahead));
                } else if vcs.behind > 0 {
                    git_extra.push_str(&format!(" \u{2193}{}", vcs.behind));
                }
            }

            let fmt = if !git_extra.is_empty() {
                if vcs.dirty {
                    format!("{}{}{}{}{}", p.label, label, p.warning, git_extra, RESET)
                } else {
                    format!("{}{}{}{}{}", p.label, label, p.border, git_extra, RESET)
                }
            } else {
                format!("{}{}{}", p.label, label, RESET)
            };
            list.push(Widget::new(fmt));
        }
    }



    list
}

fn get_metric_widgets(theme: &Theme, json: &InputJson, step: usize) -> Vec<Widget> {
    let mut list = Vec::new();
    let p = theme.palette;

    // Context window
    if step < 11 {
        let (bar_len, detail_mode) = match step {
            10 => (0, 3),
            9 => (3, 3),
            7 | 8 => (5, 3),
            6 => (6, 2),
            5 => (8, 1),
            _ => (10, 0),
        };

        let cw = json.context_window.as_ref();
        let input_tok = cw.and_then(|c| c.total_input_tokens).unwrap_or(0);
        let output_tok = cw.and_then(|c| c.total_output_tokens).unwrap_or(0);
        let limit_tok = cw.and_then(|c| c.context_window_size).unwrap_or(0);
        let base_pct = cw.and_then(|c| c.used_percentage).unwrap_or(0.0);

        let cu = cw.and_then(|c| c.current_usage.as_ref());
        let cache_read = cu.and_then(|u| u.cache_read_input_tokens).unwrap_or(0);
        let cache_create = cu.and_then(|u| u.cache_creation_input_tokens).unwrap_or(0);

        let total_used = input_tok + output_tok;
        let pct = if limit_tok > 0 {
            (total_used as f64 / limit_tok as f64) * 100.0
        } else {
            base_pct
        };

        let text_color = if pct >= 90.0 { p.critical } else if pct >= 75.0 { p.warning } else { p.accent };
        let bar_text = render_progress_bar(pct, bar_len, text_color, false);

        let detail_text = match detail_mode {
            0 => format!(" ({}/{})", get_human_format(total_used), get_human_format(limit_tok)),
            1 if limit_tok > 0 => {
                let free_pct = 100.0 - pct;
                let free_tok = limit_tok.saturating_sub(total_used);
                format!(" (free:{:.1}%/{})", free_pct, get_human_format(free_tok))
            }
            2 if total_used > 0 && limit_tok > 0 => {
                format!(" ({}/{})", get_human_format(total_used), get_human_format(limit_tok))
            }
            _ => String::new(),
        };

        let icon = theme::get_icon("context");
        let full_text = if bar_len > 0 {
            format!(
                "{}{}{}ctx{} {} {}{}{:.1}%{}{}{}{}",
                p.label, icon, p.label, RESET, bar_text, text_color, BOLD, pct, RESET, p.label, detail_text, RESET
            )
        } else {
            format!("{}{}{}ctx{} {}{}{:.1}%{}", p.label, icon, p.label, RESET, text_color, BOLD, pct, RESET)
        };
        list.push(Widget::new(full_text));

        // Cache tokens
        if step < 3 && (cache_read > 0 || cache_create > 0) {
            let rd_fmt = get_human_format(cache_read);
            let icon = theme::get_icon("cache");
            let cache_text = if cache_create > 0 {
                let wr_fmt = get_human_format(cache_create);
                format!(
                    "{}{}{}cache{} {}{}rd:{}/wr:{}{}",
                    p.label, icon, p.label, RESET, p.accent, BOLD, rd_fmt, wr_fmt, RESET
                )
            } else {
                format!(
                    "{}{}{}cache{} {}{}rd:{}{}",
                    p.label, icon, p.label, RESET, p.accent, BOLD, rd_fmt, RESET
                )
            };
            list.push(Widget::new(cache_text));
        }
    }

    // Artifacts
    let artifacts = json.artifacts.as_ref().map(|a| a.len())
        .or(json.artifact_count.map(|c| c as usize)).unwrap_or(0);
    if artifacts > 0 && step < 6 {
        let icon = theme::get_icon("artifacts");
        list.push(Widget::new(format!(
            "{}{}{}artifacts{} {}{}{}{}",
            p.label, icon, p.label, RESET, p.accent, BOLD, artifacts, RESET
        )));
    }

    // Tasks
    let tasks = json.background_tasks.as_ref().map(|t| t.len())
        .or(json.task_count.map(|c| c as usize)).unwrap_or(0);
    if tasks > 0 && step < 8 {
        let icon = theme::get_icon("tasks");
        list.push(Widget::new(format!(
            "{}{}{}tasks{} {}{}{}{}",
            p.label, icon, p.label, RESET, p.accent, BOLD, tasks, RESET
        )));
    }

    // Sandbox
    if let Some(ref sb) = json.sandbox {
        if sb.enabled.unwrap_or(false) && step < 4 {
            let icon = theme::get_icon("sandbox");
            let net_label = if sb.allow_network.unwrap_or(false) { "ON(net)" } else { "ON(no-net)" };
            list.push(Widget::new(format!(
                "{}{}{}sandbox{} {}{}{}{}",
                p.label, icon, p.label, RESET, p.success, BOLD, net_label, RESET
            )));
        }
    }

    list
}

pub fn render_tui(config: &UserConfig, json: &InputJson, cache: &CacheData) {
    let cols = json.terminal_width.unwrap_or(80);
    let theme = theme::resolve(&config.theme);
    let p = theme.palette;

    let max_w = if cols >= 80 { cols - 4 } else { cols - 2 };
    let max_metric_w = if cols >= 80 { cols - 5 } else { cols - 2 };

    let get_row_width = |widgets: &[Widget], sep_len: usize| -> usize {
        if widgets.is_empty() { return 0; }
        let total: usize = widgets.iter().map(|w| w.visual_width).sum();
        total + sep_len * (widgets.len() - 1)
    };

    let (min_info_step, min_metric_step) = if cols >= 160 {
        (0, 0)
    } else if cols >= 120 {
        (3, 0)
    } else if cols >= 80 {
        (3, 5)
    } else if cols >= 60 {
        (5, 6)
    } else {
        (6, 6)
    };

    let sep = format!(" {}|{} ", p.border, p.border);

    // Try single-line layout for wide terminals
    let mut single_line_rendered = None;
    if cols >= 160 {
        for s in min_info_step..=11 {
            let s_info = std::cmp::min(s, 6);
            let s_metric = std::cmp::min(s, 11);
            let mut combined = get_info_widgets(&theme, json, cache, s_info, cols);
            combined.extend(get_metric_widgets(&theme, json, s_metric));

            if get_row_width(&combined, 3) <= max_w && s <= 2 {
                let texts: Vec<String> = combined.into_iter().map(|w| w.text).collect();
                single_line_rendered = Some(texts.join(&sep));
                break;
            }
        }
    }

    let mut rendered_rows = Vec::new();
    if let Some(single) = single_line_rendered {
        rendered_rows.push(single);
    } else {
        // Info row
        let mut info_widgets = Vec::new();
        for s in min_info_step..=6 {
            let widgets = get_info_widgets(&theme, json, cache, s, cols);
            if get_row_width(&widgets, 3) <= max_w {
                info_widgets = widgets;
                break;
            }
            if s == 6 {
                info_widgets = widgets;
            }
        }

        // Metric row
        let mut metric_widgets = Vec::new();
        for s in min_metric_step..=11 {
            let widgets = get_metric_widgets(&theme, json, s);
            if get_row_width(&widgets, 3) <= max_metric_w {
                metric_widgets = widgets;
                break;
            }
            if s == 11 {
                metric_widgets = widgets;
            }
        }

        let info_row = info_widgets.into_iter().map(|w| w.text).collect::<Vec<String>>().join(&sep);
        rendered_rows.push(info_row);

        if !metric_widgets.is_empty() {
            let metric_row = metric_widgets.into_iter().map(|w| w.text).collect::<Vec<String>>().join(&sep);
            rendered_rows.push(metric_row);
        }
    }

    // Output with box-drawing borders
    if cols >= 80 {
        match rendered_rows.len() {
            1 => println!("{}\u{256d}\u{2500}{} {}", p.border, RESET, rendered_rows[0]),
            2 => {
                println!("{}\u{256d}\u{2500}{} {}", p.border, RESET, rendered_rows[0]);
                println!("{}\u{2570}\u{2500}{} {}", p.border, RESET, rendered_rows[1]);
            }
            n if n > 2 => {
                println!("{}\u{256d}\u{2500}{} {}", p.border, RESET, rendered_rows[0]);
                for i in 1..n - 1 {
                    println!("{}\u{251c}\u{2500}{} {}", p.border, RESET, rendered_rows[i]);
                }
                println!("{}\u{2570}\u{2500}{} {}", p.border, RESET, rendered_rows[n - 1]);
            }
            _ => {}
        }
    } else {
        for row in rendered_rows {
            println!("{}", row);
        }
    }
}
