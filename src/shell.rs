use std::ffi::OsStr;
use std::io::BufRead;
use std::time::Duration;

use futures::future::IntoFuture;
use lazy_static::lazy_static;
use props_rs::Property;
use regex::Regex;
use strum_macros::IntoStaticStr;
use tokio::sync::oneshot::Receiver;

use crate::command::{CommandBuilder, Error, ProcessResult, Result};
use crate::input::{InputSource, KeyCode, KeyEventType};
use crate::util::Vec8ToString;
use crate::{Adb, AdbDevice, Shell};

#[derive(IntoStaticStr)]
#[allow(non_camel_case_types)]
pub enum DumpsysPriority {
    CRITICAL,
    HIGH,
    NORMAL,
}

#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct ScreenRecordOptions {
    /// --bit-rate 4000000
    /// Set the video bit rate, in bits per second. Value may be specified as bits or megabits, e.g. '4000000' is equivalent to '4M'.
    /// Default 20Mbps.
    pub bitrate: Option<u64>,

    /// --time-limit=120 (in seconds)
    /// Set the maximum recording time, in seconds. Default / maximum is 180
    pub timelimit: Option<Duration>,

    /// --rotate
    /// Rotates the output 90 degrees. This feature is experimental.
    pub rotate: Option<bool>,

    /// --bugreport
    /// Add additional information, such as a timestamp overlay, that is helpful in videos captured to illustrate bugs.
    pub bug_report: Option<bool>,

    /// --size 1280x720
    /// Set the video size, e.g. "1280x720". Default is the device's main display resolution (if supported), 1280x720 if not.
    /// For best results, use a size supported by the AVC encoder.
    pub size: Option<(u16, u16)>,

    /// --verbose
    /// Display interesting information on stdout
    pub verbose: bool,
}

#[derive(IntoStaticStr)]
#[allow(non_camel_case_types)]
pub enum SettingsType {
    global,
    system,
    secure,
}

impl Shell {
    pub async fn exec<'a, D, T>(adb: &Adb, device: D, args: Vec<T>, signal: Option<IntoFuture<Receiver<()>>>) -> Result<ProcessResult>
    where
        T: Into<String> + AsRef<OsStr>,
        D: Into<&'a dyn AdbDevice>,
    {
        CommandBuilder::shell(adb, device)
            .args(args)
            .with_signal(signal)
            .output()
            .await
    }

    pub async fn list_settings<'a, D>(adb: &Adb, device: D, settings_type: SettingsType) -> Result<Vec<Property>>
    where
        D: Into<&'a dyn AdbDevice>,
    {
        let output = Shell::exec(adb, device, vec!["settings", "list", settings_type.into()], None).await?;
        let result = props_rs::parse(&output.stdout())?;
        Ok(result)
    }

    pub async fn get_setting<'a, D>(adb: &Adb, device: D, settings_type: SettingsType, key: &str) -> Result<Option<String>>
    where
        D: Into<&'a dyn AdbDevice>,
    {
        Shell::exec(adb, device, vec!["settings", "get", settings_type.into(), key], None)
            .await?
            .stdout()
            .as_str()
            .map(|s| Some(s.trim_end().to_string()))
            .ok_or(Error::from("unexpected error"))
    }

    pub async fn put_setting<'a, D>(adb: &Adb, device: D, settings_type: SettingsType, key: &str, value: &str) -> Result<()>
    where
        D: Into<&'a dyn AdbDevice>,
    {
        Shell::exec(adb, device, vec!["settings", "put", settings_type.into(), key, value], None).await?;
        Ok(())
    }

    pub async fn delete_setting<'a, D>(adb: &Adb, device: D, settings_type: SettingsType, key: &str) -> Result<()>
    where
        D: Into<&'a dyn AdbDevice>,
    {
        Shell::exec(adb, device, vec!["settings", "delete", settings_type.into(), key], None).await?;
        Ok(())
    }

    pub async fn list_dir<'a, 'b, T, D>(adb: &Adb, device: D, path: T) -> Result<Vec<String>>
    where
        T: Into<&'a str> + AsRef<OsStr>,
        D: Into<&'b dyn AdbDevice>,
    {
        let stdout = Shell::exec(adb, device, vec!["ls", "-lLHap", path.into()], None)
            .await?
            .stdout();
        let lines = stdout.lines().filter_map(|s| s.ok()).collect();
        Ok(lines)
    }

    pub async fn dumpsys_list<'a, D>(adb: &Adb, device: D, proto_only: bool, priority: Option<DumpsysPriority>) -> Result<Vec<String>>
    where
        D: Into<&'a dyn AdbDevice>,
    {
        let mut args = vec!["dumpsys", "-l"];
        if proto_only {
            args.push("--proto");
        }

        if priority.is_some() {
            args.push("--priority");
            args.push(priority.unwrap().into());
        }

        let output: Vec<String> = Shell::exec(adb, device, args, None)
            .await?
            .stdout()
            .lines()
            .filter_map(|f| match f {
                Ok(s) => {
                    let line = String::from(s.trim());
                    match line {
                        x if x.ends_with(':') => None,
                        x => Some(x),
                    }
                }
                Err(_) => None,
            })
            .collect();

        Ok(output)
    }

    pub async fn screen_record<'a, D>(adb: &Adb, device: D, options: Option<ScreenRecordOptions>, output: &str, signal: Option<IntoFuture<Receiver<()>>>) -> Result<ProcessResult>
    where
        D: Into<&'a dyn AdbDevice>,
    {
        let mut args = vec![String::from("screenrecord")];

        if options.is_some() {
            let options_args = &mut Into::<Vec<String>>::into(options.unwrap());
            args.append(options_args);
        }

        args.push(output.to_string());
        CommandBuilder::shell(adb, device)
            .args(args)
            .with_signal(signal)
            .output()
            .await
    }

    pub async fn save_screencap<'a, D>(adb: &Adb, device: D, path: &str) -> Result<ProcessResult>
    where
        D: Into<&'a dyn AdbDevice>,
    {
        Shell::exec(adb, device, vec!["screencap", "-p", path], None).await
    }

    pub async fn is_screen_on<'a, D>(adb: &Adb, device: D) -> Result<bool>
    where
        D: Into<&'a dyn AdbDevice>,
    {
        let process_result = Shell::exec(adb, device, vec!["dumpsys input_method | egrep 'mInteractive=(true|false)'"], None).await?;
        let result = process_result
            .stdout()
            .as_str()
            .map(|f| f.contains("mInteractive=true"))
            .ok_or(Error::from("unexpected error"))?;
        Ok(result)
    }

    pub async fn send_swipe<'a, D>(adb: &Adb, device: D, from_pos: (i32, i32), to_pos: (i32, i32), duration: Option<Duration>, source: Option<InputSource>) -> Result<()>
    where
        D: Into<&'a dyn AdbDevice>,
    {
        let mut args = vec!["input"];
        if source.is_some() {
            args.push(source.unwrap().into());
        }

        args.push("swipe");

        let pos_string = format!("{:?} {:?} {:?} {:?}", from_pos.0, from_pos.1, to_pos.0, to_pos.1);
        args.push(pos_string.as_str());

        #[allow(unused_assignments)]
        let mut duration_str: String = String::from("");

        if duration.is_some() {
            duration_str = duration.unwrap().as_millis().to_string();
            args.push(duration_str.as_str());
        }

        Shell::exec(adb, device, args, None).await?;
        Ok(())
    }

    pub async fn send_tap<'a, D>(adb: &Adb, device: D, position: (i32, i32), source: Option<InputSource>) -> Result<()>
    where
        D: Into<&'a dyn AdbDevice>,
    {
        let mut args = vec!["input"];
        if source.is_some() {
            args.push(source.unwrap().into());
        }

        args.push("tap");

        let pos0 = format!("{:?}", position.0);
        let pos1 = format!("{:?}", position.1);

        args.push(pos0.as_str());
        args.push(pos1.as_str());

        Shell::exec(adb, device, args, None).await?;
        Ok(())
    }

    pub async fn send_text<'a, D>(adb: &Adb, device: D, text: &str, source: Option<InputSource>) -> Result<()>
    where
        D: Into<&'a dyn AdbDevice>,
    {
        let mut args = vec!["input"];
        if source.is_some() {
            args.push(source.unwrap().into());
        }

        args.push("text");
        let formatted = format!("{:?}", text);
        args.push(formatted.as_str());

        Shell::exec(adb, device, args, None).await?;
        Ok(())
    }

    pub async fn send_event<'a, D>(adb: &Adb, device: D, event: &str, code_type: i32, code: i32, value: i32) -> Result<()>
    where
        D: Into<&'a dyn AdbDevice>,
    {
        Shell::exec(
            adb,
            device,
            vec![
                "sendevent",
                event,
                format!("{}", code_type).as_str(),
                format!("{}", code).as_str(),
                format!("{}", value).as_str(),
            ],
            None,
        )
        .await?;
        Ok(())
    }

    pub async fn get_events<'a, D>(adb: &Adb, device: D) -> Result<Vec<(String, String)>>
    where
        D: Into<&'a dyn AdbDevice>,
    {
        let result = Shell::exec(adb, device, vec!["getevent", "-S"], None)
            .await?
            .stdout();

        lazy_static! {
            static ref RE: Regex = Regex::new("^add\\s+device\\s+[0-9]+:\\s(?P<event>[^\n]+)\\s*name:\\s*\"(?P<name>[^\"]+)\"\n?").unwrap();
        }

        let mut v: Vec<(String, String)> = vec![];
        let mut string = result
            .as_str()
            .ok_or(Error::from("failed to fetch output"))?;

        loop {
            let captures = RE.captures(string);
            if captures.is_some() {
                let cap = captures.unwrap();
                let e = cap.name("event");
                let n = cap.name("name");

                if e.is_some() && n.is_some() {
                    v.push((e.unwrap().as_str().to_string(), n.unwrap().as_str().to_string()));
                }

                string = &string[cap[0].len()..]
            } else {
                break;
            }
        }
        Ok(v)
    }

    pub async fn send_keyevent<'a, D>(adb: &Adb, device: D, keycode: KeyCode, event_type: Option<KeyEventType>, source: Option<InputSource>) -> Result<()>
    where
        D: Into<&'a dyn AdbDevice>,
    {
        Shell::send_keyevents(adb, device, vec![keycode], event_type, source).await
    }

    pub async fn send_keyevents<'a, D>(adb: &Adb, device: D, keycodes: Vec<KeyCode>, event_type: Option<KeyEventType>, source: Option<InputSource>) -> Result<()>
    where
        D: Into<&'a dyn AdbDevice>,
    {
        let mut args = vec!["input"];

        if source.is_some() {
            args.push(source.unwrap().into());
        }

        args.push("keyevent");

        if event_type.is_some() {
            match event_type.unwrap() {
                KeyEventType::LongPress => args.push("--longpress"),
                KeyEventType::DoubleTap => args.push("--doubletap"),
            }
        }

        let mut code_str: Vec<&str> = keycodes
            .iter()
            .map(|k| {
                let str: &str = k.into();
                str
            })
            .collect();

        args.append(&mut code_str);

        Shell::exec(adb, device, args, None).await?;
        Ok(())
    }

    pub async fn stat<'a, D>(adb: &Adb, device: D, path: &OsStr) -> Result<file_mode::Mode>
    where
        D: Into<&'a dyn AdbDevice>,
    {
        let output = Shell::exec(adb, device, vec!["stat", "-L", "-c", "'%a'", format!("{:?}", path).as_str()], None)
            .await?
            .stdout()
            .as_str()
            .ok_or(Error::from("stat failed"))?
            .trim_end()
            .parse::<u32>()?;

        let mode = file_mode::Mode::from(output);
        Ok(mode)
    }

    async fn test_file<'a, 'b, T, D>(adb: &Adb, device: D, path: T, mode: &str) -> Result<bool>
    where
        T: Into<&'a str> + AsRef<OsStr>,
        D: Into<&'b dyn AdbDevice>,
    {
        let output = Shell::exec(adb, device, vec![format!("test -{:} {:?} && echo 1 || echo 0", mode, path.as_ref()).as_str()], None).await;

        match output?.stdout().as_str() {
            Some(s) => Ok(s.trim_end() == "1"),
            None => Ok(false),
        }
    }

    pub async fn exists<'a, 'b, T, D>(adb: &Adb, device: D, path: T) -> Result<bool>
    where
        T: Into<&'a str> + AsRef<OsStr>,
        D: Into<&'b dyn AdbDevice>,
    {
        Shell::test_file(adb, device, path, "e").await
    }

    pub async fn is_file<'a, 'b, T, D>(adb: &Adb, device: D, path: T) -> Result<bool>
    where
        T: Into<&'a str> + AsRef<OsStr>,
        D: Into<&'b dyn AdbDevice>,
    {
        Shell::test_file(adb, device, path, "f").await
    }

    pub async fn is_dir<'a, 'b, T, D>(adb: &Adb, device: D, path: T) -> Result<bool>
    where
        T: Into<&'a str> + AsRef<OsStr>,
        D: Into<&'b dyn AdbDevice>,
    {
        Shell::test_file(adb, device, path, "d").await
    }

    pub async fn is_symlink<'a, 'b, T, D>(adb: &Adb, device: D, path: T) -> Result<bool>
    where
        T: Into<&'a str> + AsRef<OsStr>,
        D: Into<&'b dyn AdbDevice>,
    {
        Shell::test_file(adb, device, path, "h").await
    }

    pub async fn getprop<'a, D>(adb: &Adb, device: D, key: &str) -> std::result::Result<Vec<u8>, Error>
    where
        D: Into<&'a dyn AdbDevice>,
    {
        Shell::exec(adb, device, vec!["getprop", key], None)
            .await
            .map(|s| s.stdout())
    }

    pub async fn getprops<'a, D>(adb: &Adb, device: D) -> Result<Vec<Property>>
    where
        D: Into<&'a dyn AdbDevice>,
    {
        let output = Shell::exec(adb, device, vec!["getprop"], None).await;

        lazy_static! {
            static ref RE: Regex = Regex::new("(?m)^\\[(.*)\\]\\s*:\\s*\\[([^\\]]*)\\]$").unwrap();
        }

        let mut result: Vec<Property> = Vec::new();

        for line in output?.stdout().lines().filter_map(|l| l.ok()) {
            let cap = RE.captures(line.as_str());
            if cap.is_some() {
                let cap1 = cap.unwrap();
                let k = cap1.get(1);
                let v = cap1.get(2);
                if k.is_some() && v.is_some() {
                    result.push(Property {
                        key: k.unwrap().as_str().to_string(),
                        value: v.unwrap().as_str().to_string(),
                    });
                }
            }
        }
        Ok(result)
    }

    pub async fn cat<'a, 'b, T, D>(adb: &Adb, device: D, path: T) -> std::result::Result<Vec<u8>, Error>
    where
        T: Into<&'a str> + AsRef<OsStr>,
        D: Into<&'b dyn AdbDevice>,
    {
        Shell::exec(adb, device, vec!["cat", path.into()], None)
            .await
            .map(|s| s.stdout())
    }

    pub async fn which<'a, D>(adb: &Adb, device: D, command: &str) -> Result<Option<String>>
    where
        D: Into<&'a dyn AdbDevice>,
    {
        let output = Shell::exec(adb, device, vec!["which", command], None).await;
        output.map(|s| s.stdout().as_str().map(|ss| String::from(ss.trim_end())))
    }

    pub async fn whoami<'a, T>(adb: &Adb, device: T) -> Result<Option<String>>
    where
        T: Into<&'a dyn AdbDevice>,
    {
        let result = Shell::exec(adb, device, vec!["whoami"], None).await?;
        Ok(result.stdout().as_str().map(|s| s.to_string()))
    }

    pub async fn is_root<'a, T>(adb: &Adb, device: T) -> Result<bool>
    where
        T: Into<&'a dyn AdbDevice>,
    {
        let whoami = Shell::whoami(adb, device).await?;
        match whoami {
            Some(s) => Ok(s.trim() == "root"),
            None => Ok(false),
        }
    }
}
