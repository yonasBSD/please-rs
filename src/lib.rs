//    please
//    Copyright (C) 2020-2021 ed neville
//
//    This program is free software: you can redistribute it and/or modify
//    it under the terms of the GNU General Public License as published by
//    the Free Software Foundation, either version 3 of the License, or
//    (at your option) any later version.
//
//    This program is distributed in the hope that it will be useful,
//    but WITHOUT ANY WARRANTY; without even the implied warranty of
//    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
//    GNU General Public License for more details.
//
//    You should have received a copy of the GNU General Public License
//    along with this program.  If not, see <http://www.gnu.org/licenses/>.

use regex::Regex;
use std::collections::HashMap;
use std::env;
use std::ffi::{CStr, CString};
use std::path::Path;
use std::process;
use syslog::{Facility, Formatter3164};

use chrono::{NaiveDate, NaiveDateTime, Utc};
use nix::sys::signal;
use nix::sys::signal::*;

use std::fmt;
use std::fs;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::time::SystemTime;
use uzers::os::unix::UserExt;
use uzers::*;

use getopts::{Matches, Options};
use nix::unistd::{alarm, gethostname, setegid, seteuid, setgid, setuid};
use pam::Authenticator;

use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum EditMode {
    Mode(i32),
    Keep(bool),
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum ReasonType {
    Need(bool),
    Text(String),
}

#[derive(Clone, Debug)]
pub struct EnvOptions {
    pub name: Option<String>,
    pub exact_name: Option<String>,
    pub rule: Option<String>,
    pub exact_rule: Option<String>,
    pub notbefore: Option<NaiveDateTime>,
    pub notafter: Option<NaiveDateTime>,
    pub datematch: Option<String>,
    pub target: Option<String>,
    pub exact_target: Option<String>,
    pub target_group: Option<String>,
    pub exact_target_group: Option<String>,
    pub hostname: Option<String>,
    pub exact_hostname: Option<String>,
    pub permit: Option<bool>,
    pub require_pass: Option<bool>,
    pub acl_type: Acltype,
    pub file_name: String,
    pub section: String,
    pub group: bool,
    pub configured: bool,
    pub dir: Option<String>,
    pub exact_dir: Option<String>,
    pub exitcmd: Option<String>,
    pub edit_mode: Option<EditMode>,
    pub reason: Option<ReasonType>,
    pub last: Option<bool>,
    pub syslog: Option<bool>,
    pub env_permit: Option<String>,
    pub env_assign: Option<HashMap<String, String>>,
    pub timeout: Option<u32>,
    pub search_path: Option<String>,
    pub token_timeout: Option<u64>,
}

impl EnvOptions {
    pub fn new() -> EnvOptions {
        EnvOptions {
            name: None,
            exact_name: None,
            rule: Some("^$".to_string()),
            exact_rule: None,
            target: Some("root".to_string()),
            exact_target: None,
            target_group: None,
            exact_target_group: None,
            notbefore: None,
            notafter: None,
            datematch: None,
            hostname: None,
            exact_hostname: None,
            file_name: "".to_string(),
            section: "".to_string(),
            permit: None,
            require_pass: None,
            acl_type: Acltype::Run,
            group: false,
            configured: false,
            dir: None,
            exact_dir: None,
            exitcmd: None,
            edit_mode: None,
            reason: None,
            last: None,
            syslog: None,
            env_permit: None,
            env_assign: None,
            timeout: None,
            search_path: None,
            token_timeout: None,
        }
    }
    fn new_deny() -> EnvOptions {
        let mut opt = EnvOptions::new();
        opt.permit = Some(false);
        opt.rule = Some(".".to_string());
        opt.target = Some("^$".to_string());
        opt.acl_type = Acltype::List;
        opt
    }
    pub fn permit(&self) -> bool {
        if self.permit.is_some() && !self.permit.unwrap() {
            return false;
        }

        true
    }
    pub fn require_pass(&self) -> bool {
        if self.require_pass.is_some() && !self.require_pass.unwrap() {
            return false;
        }

        true
    }
}

impl Default for EnvOptions {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct RunOptions {
    pub name: String,
    pub original_uid: nix::unistd::Uid,
    pub original_gid: nix::unistd::Gid,
    pub target: String,
    pub target_group: Option<String>,
    pub command: String,
    pub original_command: Vec<String>,
    pub hostname: String,
    pub directory: Option<String>,
    pub groups: HashMap<String, u32>,
    pub date: NaiveDateTime,
    pub acl_type: Acltype,
    pub reason: Option<String>,
    pub syslog: bool,
    pub prompt: bool,
    pub purge_token: bool,
    pub warm_token: bool,
    pub new_args: Vec<String>,
    pub old_umask: Option<nix::sys::stat::Mode>,
    pub old_envs: Option<HashMap<String, String>>,
    pub allow_env_list: Option<Vec<String>>,
    pub env_options: Option<EnvOptions>,
    pub cloned_args: Option<Vec<String>>,
    pub located_bin: HashMap<String, Option<String>>,
    pub resume: Option<bool>,
}

impl RunOptions {
    pub fn new() -> RunOptions {
        RunOptions {
            name: "root".to_string(),
            original_uid: nix::unistd::Uid::from_raw(get_current_uid()),
            original_gid: nix::unistd::Gid::from_raw(get_current_gid()),
            target: "".to_string(),
            target_group: None,
            command: "".to_string(),
            original_command: vec![],
            hostname: "localhost".to_string(),
            date: Utc::now().naive_utc(),
            groups: HashMap::new(),
            directory: None,
            acl_type: Acltype::Run,
            reason: None,
            syslog: true,
            prompt: true,
            purge_token: false,
            warm_token: false,
            new_args: vec![],
            old_umask: None,
            old_envs: None,
            allow_env_list: None,
            env_options: None,
            cloned_args: None,
            located_bin: HashMap::new(),
            resume: None,
        }
    }
}

impl Default for RunOptions {
    fn default() -> Self {
        Self::new()
    }
}

struct PamConvo {
    login: String,
    passwd: Option<String>,
    service: String,
}

impl pam::Converse for PamConvo {
    fn prompt_echo(&mut self, _msg: &CStr) -> Result<CString, ()> {
        CString::new(self.login.clone()).map_err(|_| ())
    }
    fn prompt_blind(&mut self, _msg: &CStr) -> Result<CString, ()> {
        match rpassword::prompt_password(format!(
            "[{}] password for {}: ",
            self.service, self.login
        )) {
            Ok(password) => self.passwd = Some(password),
            Err(_) => {
                println!("Cannot read from terminal");
                std::process::exit(1);
            }
        }

        CString::new(self.passwd.clone().unwrap()).map_err(|_| ())
    }
    fn info(&mut self, _msg: &CStr) {}
    fn error(&mut self, msg: &CStr) {
        println!("[{} pam error] {}", self.service, msg.to_string_lossy());
    }
    fn username(&self) -> &str {
        &self.login
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Acltype {
    Run,
    List,
    Edit,
}

impl fmt::Display for Acltype {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Acltype::Run => write!(f, "run"),
            Acltype::List => write!(f, "list"),
            Acltype::Edit => write!(f, "edit"),
        }
    }
}

pub fn print_may_not(ro: &RunOptions) {
    println!(
        "You may not {} \"{}\" on {} as {}",
        if ro.acl_type == Acltype::Run {
            "execute".to_string()
        } else {
            ro.acl_type.to_string()
        },
        &ro.command,
        &ro.hostname,
        &ro.target
    );
}

/// build a regex and replace %{USER} with the user str, prefix with ^ and suffix with $
pub fn regex_build(
    v: &str,
    ro: &RunOptions,
    config_path: &str,
    section: &str,
    line: Option<i32>,
) -> Option<Regex> {
    let rule = Regex::new(&format!(
        "^{}$",
        &v.replace("%{USER}", &ro.name)
            .replace("%{HOSTNAME}", &ro.hostname)
    ));
    if rule.is_err() {
        println!(
            "Error parsing {}{}",
            config_path,
            if line.is_some() {
                format!(": {}:{}", section, line.unwrap())
            } else {
                "".to_string()
            }
        );
        return None;
    }
    Some(rule.unwrap())
}

/// return true if the inclusion exists and ends with .ini
pub fn can_dir_include(file: &str) -> bool {
    let p = Path::new(file);

    if !p.is_file() {
        return false;
    }
    can_include_file_pattern(file)
}

pub fn can_include_file_pattern(file: &str) -> bool {
    let dir_pattern = Regex::new(r".*\.ini$").unwrap();

    if dir_pattern.is_match(file) {
        let p = Path::new(file);

        if p.file_name().is_none() {
            return false;
        }

        match p.file_name().unwrap().to_str() {
            None => {
                return false;
            }
            Some(f) => {
                if f.starts_with('.') {
                    return false;
                }
            }
        }
        return true;
    }
    false
}

/// print the usage
pub fn print_usage(opts: &Options, header: &str) {
    println!("usage:");
    println!("{}", opts.usage(header));
}

/// added around easter time
pub fn credits(service: &str) {
    let mut contributors = [
        "All of the Debian Rust Maintainers, and especially Sylvestre Ledru",
        "Andy Kluger, for your feedback",
        "Cyrus Wyett, jim was better than ed",
        "@unmellow, for your early testing",
        "noproto, for your detailed report",
        "pin, for work with pkgsrc",
        "Stanley Dziegiel, for ini suggestions",
        "My wife and child, for putting up with me",
        "The SUSE Security Team, especially Matthias Gerstner",
    ];

    print_version(service);

    contributors.sort_unstable();

    println!("\nWith thanks to the following teams and people, you got us where we are today.\n");
    println!("If your name is missing, or incorrect, please get in contact.\n");
    println!("In sort order:\n");

    for i in contributors.iter() {
        println!("\t{}", i);
    }

    println!("\nYou too of course, for motivating me.");
    println!("\nI thank you all for your help.\n\n\t-- Edward Neville");
}

/// common opt arguments
pub fn common_opt_arguments(
    matches: &Matches,
    opts: &Options,
    ro: &mut RunOptions,
    service: &str,
    header: &str,
) {
    ro.new_args.clone_from(&matches.free);

    if matches.opt_present("r") {
        ro.reason = Some(matches.opt_str("r").unwrap());
    }
    if matches.opt_present("t") {
        ro.target = matches.opt_str("t").unwrap();
    }
    if matches.opt_present("g") {
        ro.target_group = Some(matches.opt_str("g").unwrap());
    }
    if matches.opt_present("u") {
        ro.target = matches.opt_str("u").unwrap();
    }

    if matches.opt_str("u").is_some()
        && matches.opt_str("t").is_some()
        && matches.opt_str("t").unwrap() != matches.opt_str("u").unwrap()
    {
        println!("Cannot use -t and -u with conflicting values");
        print_usage(opts, header);
        std::process::exit(1);
    }

    if matches.opt_present("p") {
        ro.purge_token = true;
    }
    if matches.opt_present("v") {
        print_version(service);
        std::process::exit(0);
    }
    if matches.opt_present("w") {
        ro.warm_token = true;
    }

    if matches.opt_present("n") {
        ro.prompt = false;
    }

    if matches.opt_present("h") {
        if ro.new_args == ["credits"] {
            credits(service);
            std::process::exit(0);
        }

        print_usage(opts, header);
        print_version(service);
        std::process::exit(0);
    }

    if ro.purge_token {
        if !esc_privs() {
            std::process::exit(1);
        }
        remove_token(&ro.name);
        if !drop_privs(ro) {
            std::process::exit(1);
        }
        std::process::exit(0);
    }

    if ro.warm_token {
        if ro.prompt {
            challenge_password(ro, &EnvOptions::new(), service);
        }
        std::process::exit(0);
    }

    ro.hostname = gethostname()
        .expect("Failed getting hostname")
        .into_string()
        .expect("Hostname wasn't valid UTF-8");
}

/// read an ini file and traverse includes
pub fn read_ini(
    conf: &str,
    vec_eo: &mut Vec<EnvOptions>,
    ro: &RunOptions,
    fail_error: bool,
    config_path: &str,
    bytes: &mut u64,
    ini_list: &mut HashMap<String, bool>,
) -> bool {
    let parse_datetime_from_str = NaiveDateTime::parse_from_str;
    let parse_date_from_str = NaiveDate::parse_from_str;
    let mut faulty = false;
    let mut section = String::from("no section defined");
    let mut in_section = false;
    let mut opt = EnvOptions::new();

    if ini_list.contains_key(config_path) {
        println!("Error parsing already read file {}", config_path);
        return false;
    }

    ini_list.insert(config_path.to_string(), true);

    for (mut line_number, l) in conf.split('\n').enumerate() {
        line_number += 1;
        let line = l.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            in_section = true;
            section = line[1..line.len() - 1].to_string();
            if opt.configured {
                vec_eo.push(opt);
            }
            opt = EnvOptions::new();
            opt.section.clone_from(&section);
            opt.file_name = String::from(config_path);
            continue;
        }

        let equals_pos = line.find('=');
        if equals_pos.is_none() {
            continue;
        }

        let key = line[0..equals_pos.unwrap()].trim();
        let value = line[equals_pos.unwrap() + 1..].trim();

        if !in_section {
            println!("Error parsing {}:{}", config_path, line_number);
            faulty = true;
            continue;
        }

        // env_assign is a special case as the key names are not known at compile time so do not fit in the match

        if key.starts_with("env_assign.") {
            let period_pos = key.find('.');
            let env_name = key[period_pos.unwrap() + 1..].trim();
            if !value.is_empty() {
                if opt.clone().env_assign.is_none() {
                    opt.env_assign = Some(HashMap::new());
                }
                opt.env_assign
                    .as_mut()
                    .unwrap()
                    .entry(env_name.to_string())
                    .or_insert_with(|| value.to_string());
            }
            continue;
        }

        match key {
            "include" => {
                if !value.starts_with('/') {
                    println!("Includes should start with /");
                    return true;
                }
                if read_ini_config_file(value, vec_eo, ro, fail_error, bytes, ini_list) {
                    println!("Could not include file");
                    return true;
                }
                continue;
            }
            "includedir" => {
                if !value.starts_with('/') {
                    println!("Includes should start with /");
                    return true;
                }
                match fs::read_dir(value) {
                    Err(_x) => {
                        faulty = true;
                    }
                    Ok(inc) => {
                        let mut collect = vec![];
                        for file in inc {
                            collect.push(file.unwrap().path().to_str().unwrap().to_string());
                        }
                        collect.sort();
                        for file in collect {
                            if !can_dir_include(&file) {
                                continue;
                            }
                            if read_ini_config_file(&file, vec_eo, ro, fail_error, bytes, ini_list)
                            {
                                println!("Could not include file");
                                return true;
                            }
                        }
                    }
                }

                continue;
            }
            "name" => {
                opt.name = Some(value.to_string());
                opt.configured = true;
                if fail_error
                    && regex_build(value, ro, config_path, &section, Some(line_number as i32))
                        .is_none()
                {
                    faulty = true;
                }
            }
            "exact_name" => {
                opt.exact_name = Some(value.to_string());
                opt.configured = true;
            }
            "hostname" => {
                opt.hostname = Some(value.to_string());
                opt.configured = true;
                if fail_error
                    && regex_build(value, ro, config_path, &section, Some(line_number as i32))
                        .is_none()
                {
                    faulty = true;
                }
            }
            "exact_hostname" => {
                opt.exact_hostname = Some(value.to_string());
                opt.configured = true;
            }
            "target" => {
                opt.target = Some(value.to_string());
                if fail_error
                    && regex_build(value, ro, config_path, &section, Some(line_number as i32))
                        .is_none()
                {
                    faulty = true;
                }
            }
            "exact_target" => {
                opt.exact_target = Some(value.to_string());
            }
            "target_group" => {
                opt.target_group = Some(value.to_string());
                if fail_error
                    && regex_build(value, ro, config_path, &section, Some(line_number as i32))
                        .is_none()
                {
                    faulty = true;
                }
            }
            "exact_target_group" => {
                opt.exact_target_group = Some(value.to_string());
            }
            "permit" => opt.permit = Some(value == "true"),
            "require_pass" => opt.require_pass = Some(value != "false"),
            "type" => match value.to_lowercase().as_str() {
                "edit" => opt.acl_type = Acltype::Edit,
                "list" => opt.acl_type = Acltype::List,
                _ => opt.acl_type = Acltype::Run,
            },
            "group" => opt.group = value == "true",
            "regex" | "rule" => {
                opt.rule = Some(value.to_string());
                if fail_error
                    && regex_build(value, ro, config_path, &section, Some(line_number as i32))
                        .is_none()
                {
                    faulty = true;
                }
            }
            "exact_regex" | "exact_rule" => {
                opt.exact_rule = Some(value.to_string());
                opt.configured = true;
            }
            "notbefore" if value.len() == 8 => {
                opt.notbefore = Some(
                    parse_date_from_str(value, "%Y%m%d")
                        .unwrap()
                        .and_hms_opt(0, 0, 0)
                        .unwrap(),
                )
            }
            "notafter" if value.len() == 8 => {
                opt.notafter = Some(
                    parse_date_from_str(value, "%Y%m%d")
                        .unwrap()
                        .and_hms_opt(23, 59, 59)
                        .unwrap(),
                )
            }
            "notbefore" if value.len() == 14 => {
                opt.notbefore = Some(parse_datetime_from_str(value, "%Y%m%d%H%M%S").unwrap())
            }
            "notafter" if value.len() == 14 => {
                opt.notafter = Some(parse_datetime_from_str(value, "%Y%m%d%H%M%S").unwrap())
            }
            "datematch" => {
                opt.datematch = Some(value.to_string());
                if fail_error
                    && regex_build(value, ro, config_path, &section, Some(line_number as i32))
                        .is_none()
                {
                    faulty = true;
                }
            }
            "dir" => {
                opt.dir = Some(value.to_string());
                if fail_error
                    && regex_build(value, ro, config_path, &section, Some(line_number as i32))
                        .is_none()
                {
                    faulty = true;
                }
            }
            "exact_dir" => {
                opt.exact_dir = Some(value.to_string());
                if fail_error
                    && regex_build(value, ro, config_path, &section, Some(line_number as i32))
                        .is_none()
                {
                    faulty = true;
                }
            }
            "permit_env" => {
                if !value.is_empty() {
                    opt.env_permit = Some(value.to_string());
                }
            }
            "exitcmd" => {
                if !value.is_empty() {
                    opt.exitcmd = Some(value.to_string());
                }
            }
            "editmode" => {
                if !value.is_empty() {
                    if value.parse::<i16>().is_ok() {
                        opt.edit_mode = Some(EditMode::Mode(
                            i32::from_str_radix(value.trim_start_matches('0'), 8)
                                .expect("unable to parse editmode"),
                        ));
                    } else if value.to_lowercase() == "keep" {
                        opt.edit_mode = Some(EditMode::Keep(true));
                    } else {
                        println!("Could not convert {} to numerical file mode", value);
                        faulty = true;
                    }
                }
            }
            "reason" => {
                if value == "true" || value == "false" {
                    opt.reason = Some(ReasonType::Need(value == "true"));
                } else {
                    opt.reason = Some(ReasonType::Text(value.to_string()));
                }
            }
            "last" => opt.last = Some(value == "true"),
            "syslog" => opt.syslog = Some(value == "true"),
            "timeout" => {
                let timeout: Result<u32, core::num::ParseIntError> = value.parse();
                if fail_error && timeout.is_err() {
                    faulty = true;
                } else {
                    opt.timeout = Some(timeout.unwrap());
                }
            }
            "search_path" => {
                opt.search_path = Some(value.to_string());
            }
            "token_timeout" => {
                let token_timeout: Result<u64, core::num::ParseIntError> = value.parse();
                if fail_error && token_timeout.is_err() {
                    faulty = true;
                } else {
                    opt.token_timeout = Some(token_timeout.unwrap());
                }
            }
            &_ => {
                println!("Error parsing {}:{}", config_path, line_number);
                faulty = true;
            }
        }
    }

    if opt.configured {
        vec_eo.push(opt);
    }

    fail_error && faulty
}

/// read through an ini config file, appending EnvOptions to vec_eo
/// hardcoded limit of 10M for confs
pub fn read_ini_config_file(
    config_path: &str,
    vec_eo: &mut Vec<EnvOptions>,
    ro: &RunOptions,
    fail_error: bool,
    bytes: &mut u64,
    ini_list: &mut HashMap<String, bool>,
) -> bool {
    let path = Path::new(config_path);
    let display = path.display();

    let file = match File::open(path) {
        Err(why) => {
            println!("Could not open {}: {}", display, why);
            return true;
        }
        Ok(file) => file,
    };

    match nix::sys::stat::fstat(&file) {
        Err(why) => {
            println!("Could not stat {}: {}", display, why);
            return true;
        }
        Ok(stat_data) => {
            if stat_data.st_mode & libc::S_IFREG != libc::S_IFREG {
                println!("Refusing to open non-regular file");
                return true;
            }

            if (stat_data.st_mode & !libc::S_IFMT) & (0o022) != 0 {
                println!("Refusing to parse file as group or other write permission bits are set");
                return true;
            }
        }
    }

    let byte_limit = 1024 * 1024 * 10;

    if *bytes >= byte_limit {
        println!("Exiting as too much config has already been read.");
        std::process::exit(1);
    }

    let mut s = String::new();
    let reader = BufReader::new(file).take(byte_limit).read_to_string(&mut s);

    match reader {
        Ok(n) => {
            *bytes += s.as_bytes().len() as u64;
            if n >= byte_limit as usize {
                println!("Exiting as too much config has already been read.");
                std::process::exit(1);
            }
        }
        Err(why) => {
            println!("Could not read {}: {}", display, why);
            return true;
        }
    }

    read_ini(&s, vec_eo, ro, fail_error, config_path, bytes, ini_list)
}

pub fn read_ini_config_str(
    config: &str,
    vec_eo: &mut Vec<EnvOptions>,
    ro: &RunOptions,
    fail_error: bool,
    bytes: &mut u64,
    ini_list: &mut HashMap<String, bool>,
) -> bool {
    read_ini(config, vec_eo, ro, fail_error, "static", bytes, ini_list)
}

/// may we execute with this hostname
pub fn hostname_ok(item: &EnvOptions, ro: &RunOptions, line: Option<i32>) -> bool {
    if item.exact_hostname.is_some() {
        let hostname = item.exact_hostname.as_ref().unwrap();

        if hostname != &ro.hostname
            && hostname.ne(&"any".to_string())
            && hostname.ne(&"localhost".to_string())
        {
            // println!("{}: hostname mismatch: {}", item.section, hostname);
            return false;
        }
        return true;
    }

    if item.hostname.is_some() {
        let hostname_re = match regex_build(
            item.hostname.as_ref().unwrap(),
            ro,
            &item.file_name,
            &item.section,
            line,
        ) {
            Some(check) => check,
            None => {
                println!("Could not compile {}", &item.hostname.as_ref().unwrap());
                return false;
            }
        };

        if !hostname_re.is_match(&ro.hostname)
            && !hostname_re.is_match("any")
            && !hostname_re.is_match("localhost")
        {
            // println!("{}: hostname mismatch", item.section);
            return false;
        }
    }
    true
}

pub fn target_ok(item: &EnvOptions, ro: &RunOptions, line: Option<i32>) -> bool {
    if item.exact_target.is_some() {
        let exact_target = item.exact_target.as_ref().unwrap();
        if exact_target == &ro.target {
            return true;
        }

        // println!("{}: target mismatch: {} != {}", item.section, exact_target, ro.target);
        return false;
    }

    if item.target.is_some() {
        let target_re = match regex_build(
            item.target.as_ref().unwrap(),
            ro,
            &item.file_name,
            &item.section,
            line,
        ) {
            Some(check) => check,
            None => {
                println!("Could not compile {}", &item.target.as_ref().unwrap());
                return false;
            }
        };

        if target_re.is_match(&ro.target) {
            return true;
        }
        return false;
    }
    false
}

pub fn target_group_ok(item: &EnvOptions, ro: &RunOptions, line: Option<i32>) -> bool {
    if (item.target_group.is_some() || item.exact_target_group.is_some())
        && ro.target_group.is_none()
    {
        // println!("target_group is none");
        return false;
    }

    if ro.target_group.is_none() {
        // println!("target_group is none");
        return true;
    }

    if item.exact_target_group.is_some() {
        let exact_target_group = item.exact_target_group.as_ref().unwrap();
        if exact_target_group == ro.target_group.as_ref().unwrap() {
            return true;
        }

        // println!("{}: target group mismatch: {} != {}", item.section, exact_target_group, ro.target_group.as_ref().unwrap());
        return false;
    }

    if item.target_group.is_some() {
        let target_group_re = match regex_build(
            item.target_group.as_ref().unwrap(),
            ro,
            &item.file_name,
            &item.section,
            line,
        ) {
            Some(check) => check,
            None => {
                println!("Could not compile {}", &item.target_group.as_ref().unwrap());
                return false;
            }
        };

        if target_group_re.is_match(ro.target_group.as_ref().unwrap()) {
            return true;
        }
        return false;
    }
    false
}

pub fn rule_match(item: &EnvOptions, ro: &RunOptions, line: Option<i32>) -> bool {
    if item.exact_rule.is_some() {
        let exact_rule = item.exact_rule.as_ref().unwrap();
        if exact_rule == &ro.command {
            // println!("{}: exact rule match: {} == {}", item.section, exact_rule, ro.command);
            return true;
        }
        // println!("{}: exact rule mismatch: {} != {}", item.section, exact_rule, ro.command);
        return false;
    }

    if item.rule.is_some() {
        let rule_re = match regex_build(
            item.rule.as_ref().unwrap(),
            ro,
            &item.file_name,
            &item.section,
            line,
        ) {
            Some(check) => check,
            None => {
                println!("Could not compile {}", &item.rule.as_ref().unwrap());
                return false;
            }
        };

        if rule_re.is_match(&ro.command) {
            // println!("{}: item rule is match", item.section);
            // opt = item.clone();
            return true;
        }
        // println!("{}: item rule is not match", &item.rule.as_ref().unwrap());
        return false;
    }
    false
}

/// may we execute with this directory
pub fn directory_check_ok(item: &EnvOptions, ro: &RunOptions, line: Option<i32>) -> bool {
    if item.exact_dir.is_some() {
        if ro.directory.as_ref().is_none() {
            return false;
        }

        let exact_dir = item.exact_dir.as_ref().unwrap();

        if (ro.directory.as_ref()).is_some() && exact_dir != ro.directory.as_ref().unwrap() {
            return false;
        }
        return true;
    }

    if item.dir.is_some() {
        if ro.directory.as_ref().is_none() {
            return false;
        }

        let dir_re = match regex_build(
            item.dir.as_ref().unwrap(),
            ro,
            &item.file_name,
            &item.section,
            line,
        ) {
            Some(check) => check,
            None => {
                println!("Could not compile {}", &item.dir.as_ref().unwrap());
                return false;
            }
        };

        if (ro.directory.as_ref()).is_some() && !dir_re.is_match(ro.directory.as_ref().unwrap()) {
            // && ro.directory != "." {
            return false;
        }
        return true;
    }
    if ro.directory.is_some() {
        return false;
    }
    true
}

/// may we keep environment data
pub fn environment_ok(item: &EnvOptions, ro: &RunOptions, line: Option<i32>) -> bool {
    if ro.allow_env_list.is_none() {
        // println!("allow_env_list is none");
        return true;
    }

    if item.env_permit.is_none() && ro.allow_env_list.is_some() {
        // println!("env_permit is none and allow_env_list is some");
        return false;
    }

    let env_re = match regex_build(
        item.env_permit.as_ref().unwrap(),
        ro,
        &item.file_name,
        &item.section,
        line,
    ) {
        Some(check) => check,
        None => {
            println!("Could not compile {}", &item.env_permit.as_ref().unwrap());
            return false;
        }
    };

    for permit_env in ro.allow_env_list.as_ref().unwrap() {
        // println!("permit_env is {}", permit_env);
        if !env_re.is_match(permit_env) {
            // println!( "{}: skipping as not a permitted env {} vs {}",    item.section, item.env_permit.clone().unwrap(), permit_env );
            return false;
        }
    }

    true
}

/// is the RunOption valid for the dates permitted in the EnvOption
pub fn permitted_dates_ok(item: &EnvOptions, ro: &RunOptions, line: Option<i32>) -> bool {
    if item.notbefore.is_some() && item.notbefore.unwrap() > ro.date {
        // println!("{}: now is before date", item.section);
        return false;
    }

    if item.notafter.is_some() && item.notafter.unwrap() < ro.date {
        // println!("{}: now is after date", item.section);
        return false;
    }

    if item.datematch.is_some() {
        let datematch_re = match regex_build(
            item.datematch.as_ref().unwrap(),
            ro,
            &item.file_name,
            &item.section,
            line,
        ) {
            Some(check) => check,
            None => {
                println!("Could not compile {}", &item.datematch.as_ref().unwrap());
                return false;
            }
        };

        if !datematch_re.is_match(&ro.date.format("%a %e %b %T UTC %Y").to_string()) {
            // println!("{}: skipping as not a datematch {} vs {}", item.section, item.datematch.clone().unwrap(), &ro.date.format( "%a %e %b %T UTC %Y" ).to_string() );
            return false;
        }
    }
    true
}

pub fn name_matches(item: &EnvOptions, ro: &RunOptions, line: Option<i32>) -> bool {
    if item.exact_name.is_some() {
        let name = item.exact_name.as_ref().unwrap();
        if name == &ro.name {
            return true;
        }
        //println!("{}: exact name mismatch: {} != {}", item.section, name, ro.name);
        return false;
    }

    if item.name.is_some() {
        let name_re = match regex_build(
            item.name.as_ref().unwrap(),
            ro,
            &item.file_name,
            &item.section,
            line,
        ) {
            Some(check) => check,
            None => {
                println!("Could not compile {}", &item.name.as_ref().unwrap());
                return false;
            }
        };

        if name_re.is_match(&ro.name) {
            // println!("{}: skipping as not a name match ({}), group={}", item.section, item.name.as_ref().unwrap(), item.group);
            return true;
        }

        return false;
    }
    false
}

pub fn group_matches(item: &EnvOptions, ro: &RunOptions, line: Option<i32>) -> bool {
    if item.exact_name.is_some() {
        let name = item.exact_name.as_ref().unwrap();
        for (k, _) in ro.groups.iter() {
            if name == k {
                // println!("{}: {} matches group {}", &item.section, name, k);
                return true;
            }
        }
        // println!("{}: does not match group", item.section);
        return false;
    }

    if item.name.is_some() {
        let name_re = match regex_build(
            item.name.as_ref().unwrap(),
            ro,
            &item.file_name,
            &item.section,
            line,
        ) {
            Some(check) => check,
            None => {
                println!("Could not compile {}", &item.name.as_ref().unwrap());
                return false;
            }
        };

        for (k, _) in ro.groups.iter() {
            if name_re.is_match(k) {
                // println!("{}: {} matches group {}", item.section, item.name.as_ref().unwrap(), k);
                return true;
            }
        }
        return false;
    }

    false
}

pub fn matching(item: &EnvOptions, ro: &mut RunOptions, line_error: Option<i32>) -> bool {
    if !permitted_dates_ok(item, ro, line_error) {
        // println!("Didn't match permitted dates");
        return false;
    }

    if !item.group && !name_matches(item, ro, line_error) {
        // println!("not item group, and name does not match");
        return false;
    }

    if item.group && !group_matches(item, ro, line_error) {
        // println!("item group, and group does not match");
        return false;
    }

    if !hostname_ok(item, ro, line_error) {
        // println!("hostname does not match");
        return false;
    }

    if !directory_check_ok(item, ro, line_error) {
        // println!("directory does not match");
        return false;
    }

    if !environment_ok(item, ro, line_error) {
        // println!("environment does not match");
        return false;
    }

    if !target_ok(item, ro, line_error) {
        // println!("target user does not match");
        return false;
    }

    if !target_group_ok(item, ro, line_error) {
        // println!("target group does not match");
        return false;
    }

    if item.acl_type == Acltype::List {
        // println!("{}: is list", item.section);
        return true;
    }

    // cloned_args and command should be reset each loop
    // search_path could expose privilege paths that may appear
    // in error messaging

    // shouldn't matter if setuid running user happened first, but even
    // so better than to be sorry later
    if ro.cloned_args.is_some() {
        ro.cloned_args = None;
    }
    ro.command = replace_new_args(ro.new_args.clone());

    if item.acl_type == Acltype::Run {
        match search_path(ro, item) {
            None => {
                return false;
            }
            Some(x) => {
                ro.cloned_args = Some(ro.new_args.clone());
                ro.cloned_args.as_mut().unwrap()[0] = x;
                ro.command = replace_new_args(ro.cloned_args.as_ref().unwrap().clone());
            }
        }
    }
    if item.acl_type == Acltype::Edit {
        let edit_file = vec![ro.new_args[0].clone()];
        ro.command = replace_new_args(edit_file);
    }

    rule_match(item, ro, line_error)
}

pub fn merge_default(default: &EnvOptions, item: &EnvOptions) -> EnvOptions {
    let mut merged = item.clone();

    if default.syslog.is_some() && item.syslog.is_none() {
        // println!("merging syslog");
        merged.syslog = default.syslog;
    }

    if default.reason.is_some() && item.reason.is_none() {
        // println!("merging reason");
        merged.reason.clone_from(&default.reason);
    }

    if default.require_pass.is_some() && item.require_pass.is_none() {
        // println!("merging require_pass");
        merged.require_pass = default.require_pass;
    }

    if default.last.is_some() && item.last.is_none() {
        // println!("merging last");
        merged.last = default.last;
    }

    if default.exitcmd.is_some() && item.exitcmd.is_none() {
        // println!("merging exitcmd");
        merged.exitcmd.clone_from(&default.exitcmd);
    }

    if default.edit_mode.is_some() && item.edit_mode.is_none() {
        // println!("merging edit_mode");
        merged.edit_mode.clone_from(&default.edit_mode);
    }

    if default.timeout.is_some() && item.timeout.is_none() {
        // println!("merging timeout");
        merged.timeout = default.timeout;
    }

    if default.env_permit.is_some() && item.env_permit.is_none() {
        // println!("merging env_permit");
        merged.env_permit.clone_from(&default.env_permit);
    }

    if default.env_assign.is_some() && item.env_assign.is_none() {
        // println!("merging env_assign");
        merged.env_assign.clone_from(&default.env_assign);
    }

    if default.permit.is_some() && item.permit.is_none() {
        // println!("merging permit");
        merged.permit = default.permit;
    }

    if default.search_path.is_some() && item.search_path.is_none() {
        // println!("merging search_path");
        merged.search_path.clone_from(&default.search_path);
    }

    if default.token_timeout.is_some() && item.token_timeout.is_none() {
        // println!("merging token_timeout");
        merged.token_timeout = default.token_timeout;
    }

    merged
}

/// search the EnvOptions list for matching RunOptions and return the match
pub fn can(vec_eo: &[EnvOptions], ro: &mut RunOptions) -> EnvOptions {
    let mut opt = EnvOptions::new_deny();
    let mut default = EnvOptions::new();

    for item in vec_eo {
        // println!("{}:", item.section);
        if item.acl_type != ro.acl_type {
            // println!("{}: not {:?} != {:?}", item.section, item.acl_type, ro.acl_type);
            continue;
        }

        if !matching(item, ro, None) {
            // println!("!matching");
            continue;
        }

        if item.section.starts_with("default") {
            default = merge_default(&default, item);
        }

        opt = merge_default(&default, item);

        match opt.last {
            None => {}
            Some(last) => {
                if last {
                    break;
                }
            }
        }
        // println!("didn't match");
    }
    opt
}

/// check reason. this happens post authorize in order to provide feedback
pub fn reason_ok(item: &EnvOptions, ro: &RunOptions) -> bool {
    if item.reason.is_none() {
        return true;
    }

    match &item.reason.as_ref().unwrap() {
        ReasonType::Text(value) => {
            let m_re = match regex_build(value, ro, &item.file_name, &item.section, None) {
                Some(check) => check,
                None => {
                    println!("Could not compile {}", &value);
                    return false;
                }
            };

            if ro.reason.is_some() && m_re.is_match(ro.reason.as_ref().unwrap()) {
                return true;
            }

            println!(
                "Sorry but there is no reason match to {} \"{}\" on {} as {}",
                &ro.acl_type, &ro.command, &ro.hostname, &ro.target
            );

            false
        }
        ReasonType::Need(value) => {
            if value == &true && ro.reason.is_none() {
                println!(
                    "Sorry but no reason was given to {} \"{}\" on {} as {}",
                    &ro.acl_type,
                    if ro.acl_type == Acltype::List {
                        &ro.target
                    } else {
                        &ro.command
                    },
                    &ro.hostname,
                    &ro.target
                );
                return false;
            }
            true
        }
    }
}

/// find editor for user. return /usr/bin/vi if EDITOR and VISUAL are unset
pub fn get_editor() -> String {
    let editor = "/usr/bin/vi";

    for prog in [String::from("VISUAL"), String::from("EDITOR")].iter() {
        if let Ok(val) = std::env::var(prog) {
            return val;
        }
    }

    editor.to_string()
}

/// handler.authenticate without the root privs part for linux
#[cfg(target_os = "linux")]
pub fn handler_shim<T: pam::Converse>(
    _ro: &RunOptions,
    handler: &mut Authenticator<T>,
) -> Result<(), pam::PamError> {
    handler.authenticate()
}

/// handler.authenticate needs esc_privs on netbsd
#[cfg(not(target_os = "linux"))]
pub fn handler_shim<T: pam::Converse>(
    ro: &RunOptions,
    handler: &mut Authenticator<T>,
) -> Result<(), pam::PamError> {
    if !esc_privs() {
        std::process::exit(1);
    }
    let auth = handler.authenticate();
    if !drop_privs(&ro) {
        std::process::exit(1);
    }
    auth
}

/// read password of user via rpassword
/// should pam require a password, and it is successful, then we set a token
pub fn challenge_password(ro: &RunOptions, entry: &EnvOptions, service: &str) -> bool {
    if entry.require_pass() {
        if tty_name().is_none() {
            println!("Cannot read password without tty");
            return false;
        }

        let mut retry_counter = 0;

        if !esc_privs() {
            std::process::exit(1);
        }

        if valid_token(&ro.name, entry) {
            update_token(&ro.name);
            return true;
        }

        if !drop_privs(ro) {
            std::process::exit(1);
        }

        if !ro.prompt {
            return false;
        }

        let convo = PamConvo {
            login: ro.name.to_string(),
            passwd: None,
            service: service.to_string(),
        };

        if entry.timeout.is_some() {
            extern "C" fn alarm_signal_handler(_: nix::libc::c_int) {
                println!("Timed out getting password");

                let tty = std::fs::File::open("/dev/tty");
                if tty.is_ok() {
                    let term_res = nix::sys::termios::tcgetattr(tty.as_ref().unwrap());
                    if let Ok(mut term) = term_res {
                        term.local_flags
                            .set(nix::sys::termios::LocalFlags::ECHO, true);

                        let res = nix::sys::termios::tcsetattr(
                            tty.as_ref().unwrap(),
                            nix::sys::termios::SetArg::TCSANOW,
                            &term,
                        );
                        if res.is_err() {
                            println!("Couldn't return terminal to original settings");
                        }
                    }
                }

                std::process::exit(1);
            }

            let sa = SigAction::new(
                SigHandler::Handler(alarm_signal_handler),
                SaFlags::SA_RESTART,
                SigSet::empty(),
            );

            unsafe {
                match sigaction(Signal::SIGALRM, &sa) {
                    Ok(_) => {}
                    Err(_) => {
                        println!("Couldn't reset alarm");
                        std::process::exit(1);
                    }
                }
            }
        }

        let mut handler = Authenticator::with_handler(service, convo).expect("Cannot init PAM");

        loop {
            if let Some(timeout) = entry.timeout {
                alarm::set(timeout);
            }

            let auth = handler_shim(ro, &mut handler);

            if entry.timeout.is_some() {
                alarm::cancel();
            }

            if auth.is_ok() {
                if handler.get_handler().passwd.is_some() {
                    unsafe { signal::signal(signal::SIGALRM, signal::SigHandler::SigDfl).unwrap() };
                    if !esc_privs() {
                        std::process::exit(1);
                    }

                    update_token(&ro.name);

                    if !drop_privs(ro) {
                        std::process::exit(1);
                    }
                }
                return true;
            }
            retry_counter += 1;
            if retry_counter == 3 {
                println!("Authentication failed :-(");

                return false;
            }
        }
    }
    true
}

/// return rule or exact_rule
pub fn list_rule(eo: &EnvOptions) -> String {
    if eo.exact_rule.is_some() {
        return format!("exact({})", eo.exact_rule.as_ref().unwrap());
    }
    if eo.rule.is_some() {
        return eo.rule.as_ref().unwrap().to_string();
    }
    "".to_string()
}

/// return target or exact_target
pub fn list_target(eo: &EnvOptions) -> String {
    if eo.exact_target.is_some() {
        return format!("exact({})", eo.exact_target.as_ref().unwrap());
    }
    if eo.target.is_some() {
        return eo.target.as_ref().unwrap().to_string();
    }
    "".to_string()
}

/// return dir or exact_dir
pub fn list_dir(eo: &EnvOptions) -> String {
    if eo.exact_dir.is_some() {
        return format!("exact({})", eo.exact_dir.as_ref().unwrap());
    }
    if eo.dir.is_some() {
        return eo.dir.as_ref().unwrap().to_string();
    }
    "".to_string()
}

/// print output list of acl
pub fn list(vec_eo: &[EnvOptions], ro: &RunOptions) {
    //let mut str_list: vec![];
    for s in produce_list(vec_eo, ro) {
        println!("{}", s);
    }
}

/// return EnvOptions as a vector of strings
pub fn produce_list(vec_eo: &[EnvOptions], ro: &RunOptions) -> Vec<String> {
    let mut str_list = vec![];
    let mut ro = ro.clone();

    if !ro.target.is_empty() {
        ro.name.clone_from(&ro.target);
    }

    let mut last_file = "";

    for item in vec_eo {
        if !item.group && !name_matches(item, &ro, None) {
            continue;
        }

        if item.group && !group_matches(item, &ro, None) {
            continue;
        }

        let mut prefixes = vec![];
        if item.notbefore.is_some() && item.notbefore.unwrap() > ro.date {
            prefixes.push(format!("upcomming({})", item.notbefore.unwrap()));
        }

        if item.notafter.is_some() && item.notafter.unwrap() < ro.date {
            prefixes.push(format!("expired({})", item.notafter.unwrap()));
        }

        match &item.reason {
            Some(r) => {
                if *r != ReasonType::Need(false) {
                    prefixes.push(String::from("reason_required"));
                }
            }
            None => {}
        }

        if item.acl_type != ro.acl_type {
            continue;
        }

        if !item.permit() {
            prefixes.push(String::from("not permitted"));
        }

        if !hostname_ok(item, &ro, None) {
            continue;
        }

        if item.last.is_some() && item.last.unwrap() {
            prefixes.push(String::from("last"));
        }

        let mut prefix = prefixes.join(", ");
        if !prefix.is_empty() {
            if item.acl_type != Acltype::List {
                prefix = format!(" {} as ", prefix);
            } else {
                prefix = format!(" {} to ", prefix);
            }
        }
        if last_file != item.file_name {
            str_list.push(format!("  in file: {}", item.file_name));
            last_file = &item.file_name;
        }

        if item.acl_type == Acltype::List {
            str_list.push(format!(
                "    {}:{}list: {}",
                item.section,
                prefix,
                item.target.as_ref().unwrap()
            ));
            continue;
        }

        str_list.push(format!(
            "    {}:{}{} (pass={},dirs={}): {}",
            item.section,
            prefix,
            list_target(item),
            item.require_pass(),
            list_dir(item),
            list_rule(item)
        ));
    }
    str_list
}

/// return result from search cache lookup
pub fn search_path_cache(ro: &RunOptions, binary: &str) -> Option<String> {
    match ro.located_bin.get(binary) {
        Some(k) => {
            if k.is_none() {
                // println!("{} returning None (cached lookup)", item.section);
                return None;
            }
            // println!("{} returning Some({})", item.section, k.as_ref().unwrap().to_string());
            return Some(k.as_ref().unwrap().to_string());
        }
        None => {
            // println!("{} returning None (not a cached lookup)", item.section);
            None
        }
    }
}

/// if binary is not an absolute/relative path, look for it in usual places
pub fn search_path(ro: &mut RunOptions, item: &EnvOptions) -> Option<String> {
    let binary = &ro.new_args[0];
    let p = Path::new(&binary);
    // println!("Searching for {binary}");

    if binary.starts_with('/') || binary.starts_with("./") {
        let lookup = search_path_cache(ro, binary);
        if lookup.is_some() {
            return lookup;
        }

        if !p.exists() {
            ro.located_bin.insert(binary.to_string(), None);
            return None;
        } else {
            ro.located_bin
                .insert(binary.to_string(), Some(binary.to_string()));
            return Some(binary.to_string());
        }
    }

    let dirs = if item.search_path.is_some() {
        item.search_path.as_ref().unwrap()
    } else {
        "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
    };

    for dir in dirs.split(':') {
        if dir.trim() == "" {
            continue;
        }
        let dir = dir.trim_end_matches('/');
        let path_name = format!("{}/{}", &dir, &binary);

        if let Some(lookup) = search_path_cache(ro, binary) {
            return Some(lookup);
        }

        let p = Path::new(&path_name);

        if !p.exists() {
            ro.located_bin.insert(binary.to_string(), None);
            continue;
        }

        // println!("inserting {binary} = {}", path_name.clone());
        ro.located_bin
            .insert(binary.to_string(), Some(path_name.clone()));
        return Some(path_name);
    }

    None
}

/// clean environment aside from ~half a dozen vars
pub fn clean_environment(ro: &mut RunOptions) {
    ro.old_umask = Some(nix::sys::stat::umask(
        nix::sys::stat::Mode::from_bits(0o077).unwrap(),
    ));

    for (key, val) in std::env::vars() {
        if ro.acl_type == Acltype::Edit {
            if ro.old_envs.is_none() {
                ro.old_envs = Some(HashMap::new());
            }

            ro.old_envs
                .as_mut()
                .unwrap()
                .entry(key.to_string())
                .or_insert(val);
        }

        if key == "LANGUAGE"
            || key == "XAUTHORITY"
            || key == "LANG"
            || key == "LS_COLORS"
            || key == "TERM"
            || key == "DISPLAY"
            || key == "LOGNAME"
        {
            continue;
        }

        let mut skip = false;

        if ro.allow_env_list.is_some() {
            for env in ro.allow_env_list.as_ref().unwrap() {
                if key == *env {
                    skip = true;
                    break;
                }
            }
        }
        if skip {
            continue;
        }

        if ro.acl_type == Acltype::Edit && (key == "EDITOR" || key == "VISUAL") {
            continue;
        }
        std::env::remove_var(key);
    }
}

/// set the environment unless it is permitted to be kept and is specified
pub fn set_env_if_not_passed_through(ro: &RunOptions, key: &str, value: &str) {
    if ro.allow_env_list.is_some() {
        for env in ro.allow_env_list.as_ref().unwrap() {
            if key == *env {
                // println!("Returning as {} = {}", key, *env );
                return;
            }
        }
    }

    std::env::set_var(key, value);
}

/// set environment for helper scripts
pub fn set_environment(
    ro: &RunOptions,
    entry: &EnvOptions,
    original_user: &User,
    original_uid: u32,
    lookup_name: &User,
) {
    std::env::set_var("PLEASE_USER", original_user.name());
    std::env::set_var("PLEASE_UID", original_uid.to_string());
    std::env::set_var("PLEASE_GID", original_user.primary_group_id().to_string());
    std::env::set_var("PLEASE_COMMAND", &ro.command);

    std::env::set_var("SUDO_USER", original_user.name());
    std::env::set_var("SUDO_UID", original_uid.to_string());
    std::env::set_var("SUDO_GID", original_user.primary_group_id().to_string());
    std::env::set_var("SUDO_COMMAND", &ro.command);

    set_env_if_not_passed_through(
        ro,
        "PATH",
        "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
    );

    set_env_if_not_passed_through(ro, "HOME", lookup_name.home_dir().to_str().unwrap());
    set_env_if_not_passed_through(ro, "MAIL", &format!("/var/mail/{}", ro.target));
    set_env_if_not_passed_through(ro, "SHELL", lookup_name.shell().to_str().unwrap());
    set_env_if_not_passed_through(ro, "USER", &ro.target);
    set_env_if_not_passed_through(ro, "LOGNAME", &ro.target);

    if entry.env_assign.is_some() {
        for (k, v) in entry.env_assign.as_ref().unwrap() {
            std::env::set_var(k, v);
        }
    }
}

pub fn bad_priv_msg() {
    println!("I cannot set privs. Exiting as not installed correctly.");
}

/// set privs of usr to target_uid and target_gid. return false if fails
pub fn set_privs(user: &str, target_uid: nix::unistd::Uid, target_gid: nix::unistd::Gid) -> bool {
    let name = CString::new(user.as_bytes()).unwrap();

    unsafe {
        if libc::setgroups(0, std::ptr::null()) != 0 {
            bad_priv_msg();
            return false;
        }

        #[cfg(not(target_os = "macos"))]
        if libc::initgroups(name.as_ptr(), target_gid.as_raw()) != 0 {
            bad_priv_msg();
            return false;
        }

        #[cfg(target_os = "macos")]
        if libc::initgroups(name.as_ptr(), target_gid.as_raw() as i32) != 0 {
            bad_priv_msg();
            return false;
        }
    }

    if setgid(target_gid).is_err() {
        bad_priv_msg();
        return false;
    }

    if setuid(target_uid).is_err() {
        bad_priv_msg();
        return false;
    }
    true
}

/// set privs of usr to target_uid and target_gid. return false if fails
pub fn set_eprivs(target_uid: nix::unistd::Uid, target_gid: nix::unistd::Gid) -> bool {
    if setegid(target_gid).is_err() {
        bad_priv_msg();
        return false;
    }
    if seteuid(target_uid).is_err() {
        bad_priv_msg();
        return false;
    }

    true
}

/// set privs (just call eprivs based on ro)
pub fn drop_privs(ro: &RunOptions) -> bool {
    esc_privs() && set_eprivs(ro.original_uid, ro.original_gid)
}

/// reset privs (just call eprivs based on root)
pub fn esc_privs() -> bool {
    set_eprivs(nix::unistd::Uid::from_raw(0), nix::unistd::Gid::from_raw(0))
}

/// return our best guess of what the user's tty is
pub fn tty_name() -> Option<String> {
    let mut ttyname = None;

    /* sometimes a tty isn't attached for all pipes FIXME: make this testable */
    for n in 0..3 {
        let ptr;
        unsafe {
            ptr = libc::ttyname(n);
        }
        if ptr.is_null() {
            continue;
        }

        let s;
        unsafe {
            s = CStr::from_ptr(ptr).to_str();
        }
        match s {
            Err(_x) => ttyname = None,
            Ok(x) => ttyname = Some(x.to_string()),
        }
        break;
    }

    ttyname
}

/// add a level of escape to strings when they go to the old as " holds entities
pub fn escape_log(message: &str) -> String {
    message.replace('\"', "\\\"")
}

/// write to syslog a standard log
pub fn log_action(service: &str, result: &str, ro: &RunOptions, command: &str) -> bool {
    if !ro.syslog {
        return false;
    }

    let formatter = Formatter3164 {
        facility: Facility::LOG_USER,
        hostname: None,
        process: service.into(),
        pid: process::id(),
    };

    let cwd = match env::current_dir() {
        Err(_) => "unable to get cwd".to_string(),
        Ok(x) => x.to_string_lossy().to_string(),
    };

    let matching_env = match &ro.env_options {
        Some(env_options) => {
            format!("{}:{}", env_options.file_name, env_options.section)
        }
        None => "".to_string(),
    };

    match syslog::unix(formatter) {
        Err(_e) => println!("Could not connect to syslog"),
        Ok(mut writer) => {
            let tty_name = tty_name();

            writer
                .err(format!(
                    "user=\"{}\" cwd=\"{}\" tty=\"{}\" action=\"{}\" target=\"{}\" type=\"{}\" reason=\"{}\" command=\"{}\" matching_section=\"{}\"",
                    escape_log( &ro.name ),
                    escape_log( &cwd ),
                    if tty_name.is_none() {
                        "no_tty".to_string()
                    } else {
                        tty_name.unwrap()
                    },
                    result,
                    escape_log( &ro.target ),
                    ro.acl_type,
                    if ro.reason.as_ref().is_some() {
                        escape_log( ro.reason.as_ref().unwrap() )
                    } else {
                        String::from("")
                    },
                    escape_log( command ),
                    matching_env,
                ))
                .expect("could not write error message");
        }
    }
    false
}

/// return the directory that the token should use
pub fn token_dir() -> String {
    "/var/run/please/token".to_string()
}

/// return the path of the users token
pub fn token_path(user: &str) -> Option<String> {
    let tty_name = tty_name();
    tty_name.as_ref()?;
    let ppid = nix::unistd::getppid();
    Some(format!(
        "{}/{}:{}:{}",
        token_dir(),
        user,
        tty_name.unwrap().replace('/', "_"),
        ppid
    ))
}

pub fn create_token_dir() -> bool {
    if !Path::new(&token_dir()).is_dir() && fs::create_dir_all(token_dir()).is_err() {
        println!("Could not create token directory");
        return false;
    }

    true
}

pub fn boot_secs() -> libc::timespec {
    let mut tp = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    #[cfg(target_os = "linux")]
    unsafe {
        libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut tp)
    };
    #[cfg(not(target_os = "linux"))]
    unsafe {
        libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut tp)
    };
    tp
}

/// does the user have a valid token
/// return false if time stamp is in the future
/// return true if token was set within 600 seconds of wall and boot time
pub fn valid_token(user: &str, eo: &EnvOptions) -> bool {
    if !create_token_dir() {
        return false;
    }

    let token_path = token_path(user);
    if token_path.is_none() {
        return false;
    }

    let secs = eo.token_timeout.unwrap_or(600);

    let token_path = token_path.unwrap();
    match fs::metadata(token_path) {
        Ok(meta) => {
            match meta.modified() {
                Ok(t) => {
                    let tp = boot_secs();

                    match t.duration_since(SystemTime::UNIX_EPOCH) {
                        Ok(s) => {
                            if (tp.tv_sec as u64) < s.as_secs() {
                                // println!("tv_sec lower {} vs {}", tp.tv_sec, s.as_secs());
                                return false;
                            }
                            if ((tp.tv_sec as u64) - s.as_secs()) < secs {
                                // check the atime isn't older than 600 too

                                match SystemTime::now().duration_since(meta.accessed().unwrap()) {
                                    Ok(a) => return a.as_secs() <= secs,
                                    Err(_) => return false,
                                }
                            }
                        }
                        Err(_) => {
                            return false;
                        }
                    }

                    false
                }
                Err(_e) => false,
            }
        }
        Err(_) => false,
    }
}

/// touch the users token on disk
pub fn update_token(user: &str) {
    if !create_token_dir() {
        return;
    }

    let token_path = token_path(user);
    if token_path.is_none() {
        return;
    }

    let old_mode = nix::sys::stat::umask(nix::sys::stat::Mode::from_bits(0o077).unwrap());
    let token_path = token_path.unwrap();
    let token_path_tmp = format!("{}.tmp", &token_path);
    match fs::File::create(&token_path_tmp) {
        Ok(_x) => {}
        Err(x) => println!("Error creating token: {}", x),
    }
    nix::sys::stat::umask(old_mode);

    let tp = boot_secs();

    let tv_mtime = nix::sys::time::TimeVal::from(libc::timeval {
        tv_sec: tp.tv_sec,
        tv_usec: 0,
    });

    // https://github.com/rust-lang/libc/issues/1848
    #[cfg_attr(target_env = "musl", allow(deprecated))]
    let tv_atime = nix::sys::time::TimeVal::from(libc::timeval {
        tv_sec: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as libc::time_t,
        tv_usec: 0,
    });

    if nix::sys::stat::utimes(Path::new(&token_path_tmp), &tv_atime, &tv_mtime).is_err() {
        return;
    }

    if std::fs::rename(token_path_tmp.as_str(), token_path).is_err() {
        println!("Could not update token");
    }
}

/// remove from disk the users token
pub fn remove_token(user: &str) {
    if !create_token_dir() {
        return;
    }

    let token_location = token_path(user);
    if token_location.is_none() {
        return;
    }

    let token_location = token_location.unwrap();

    let p = Path::new(&token_location);
    if p.is_file() {
        match fs::remove_file(p) {
            Ok(_x) => {}
            Err(x) => println!("Error removing token {}: {}", p.to_str().unwrap(), x),
        }
    }
}

/// turn group list into an indexed list
pub fn group_hash(groups: Vec<Group>) -> HashMap<String, u32> {
    let mut hm: HashMap<String, u32> = HashMap::new();
    for group in groups {
        hm.entry(String::from(group.name().to_string_lossy()))
            .or_insert_with(|| group.gid());
    }
    hm
}

/// escape '\' within an argument
/// escape ' ' within an argument
pub fn replace_new_args(new_args: Vec<String>) -> String {
    let mut args = vec![];
    for arg in &new_args {
        args.push(arg.replace('\\', r"\\").replace(' ', r"\ "));
    }

    args.join(" ")
}

/// print version string
pub fn print_version(program: &str) {
    println!("{} version {}", &program, env!("CARGO_PKG_VERSION"));
}

/// return a lump of random alpha numeric characters
pub fn prng_alpha_num_string(n: usize) -> String {
    let rng = thread_rng();
    rng.sample_iter(&Alphanumeric)
        .take(n)
        .map(|x| x as char)
        .collect()
}

pub fn runopt_target_gid(ro: &RunOptions, lookup_name: &uzers::User) -> nix::unistd::Gid {
    if ro.target_group.is_some() {
        match nix::unistd::Group::from_name(ro.target_group.as_ref().unwrap()) {
            Ok(x) => match x {
                Some(g) => g.gid,
                None => {
                    println!("Cannot assign group {}", &ro.target_group.as_ref().unwrap());
                    std::process::exit(1);
                }
            },
            Err(_) => {
                println!("Cannot assign group {}", &ro.target_group.as_ref().unwrap());
                std::process::exit(1);
            }
        }
    } else {
        nix::unistd::Gid::from_raw(lookup_name.primary_group_id())
    }
}
