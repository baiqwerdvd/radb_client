use std::ffi::OsStr;
use std::fmt::{Debug, Formatter};
use std::io::BufRead;
use std::path::PathBuf;
use std::process::Output;
use std::time::Duration;

use crossbeam_channel::Receiver;
use lazy_static::lazy_static;
use regex::Regex;
use simple_cmd::{Cmd, CommandBuilder};
use which::which;

use crate::v2::prelude::*;
use crate::v2::types::{Adb, AdbDevice, AddressType};

impl Adb {
    pub fn new() -> crate::v2::result::Result<Adb> {
        let adb = which("adb")?;
        Ok(Adb(adb))
    }

    pub fn exec<'a, C, T>(
        &self,
        addr: C,
        args: Vec<T>,
        cancel: Option<Receiver<()>>,
        timeout: Option<Duration>,
        debug: bool,
    ) -> crate::Result<Output>
        where
            T: Into<String> + AsRef<OsStr>,
            C: Into<AddressType>,
    {
        let builder = CommandBuilder::adb(&self)
            .addr(addr)
            .with_debug(debug)
            .args(args)
            .signal(cancel)
            .timeout(timeout);
        Ok(builder.build().output()?)
    }

    pub fn mdns_check(&self, debug: bool) -> crate::v2::result::Result<()> {
        let _output = CommandBuilder::adb(&self)
            .with_debug(debug)
            .args(&[
                "mdns", "check",
            ])
            .build()
            .output()?;
        Ok(())
    }

    /// List connected devices
    pub fn list_devices(&self, debug: bool) -> crate::v2::result::Result<Vec<AdbDevice>> {
        let output = Cmd::builder(self.0.as_path())
            .args([
                "devices", "-l",
            ])
            .with_debug(debug)
            .build()
            .output()?;

        lazy_static! {
			static ref RE: Regex = Regex::new(
				"(?P<ip>[^\\s]+)[\\s]+(device|offline) product:(?P<device_product>[^\\s]+)\\smodel:(?P<model>[^\\s]+)\\sdevice:(?P<device>[^\\s]+)\\stransport_id:(?P<transport_id>[^\\s]+)"
			)
			.unwrap();
		}

        let mut devices: Vec<AdbDevice> = vec![];
        for line in output.stdout.lines() {
            let line_str = line?;

            if RE.is_match(line_str.as_str()) {
                let captures = RE.captures(line_str.as_str());
                match captures {
                    None => {}
                    Some(c) => {
                        let ip = c.name("ip").unwrap().as_str();
                        let product = c.name("device_product").unwrap().as_str();
                        let model = c.name("model").unwrap().as_str();
                        let device = c.name("device").unwrap().as_str();
                        let tr = c.name("transport_id").unwrap().as_str().parse::<u8>().unwrap();

                        if let Ok(d) = match AddressType::try_from_ip(ip) {
                            Ok(addr) => Ok::<AddressType, crate::v2::error::Error>(addr),
                            Err(_) => Ok(AddressType::Transport(tr)),
                        } {
                            let device = AdbDevice {
                                name: ip.to_string(),
                                product: product.to_string(),
                                model: model.to_string(),
                                device: device.to_string(),
                                addr: d,
                            };
                            devices.push(device)
                        }
                    }
                }
            }
        }
        Ok(devices)
    }

    pub fn as_os_str(&self) -> &OsStr {
        self.as_ref()
    }
}

impl std::fmt::Display for Adb {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0.to_str())
    }
}

impl Debug for Adb {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<OsStr> for Adb {
    fn as_ref(&self) -> &OsStr {
        self.0.as_ref()
    }
}

impl Into<PathBuf> for Adb {
    fn into(self) -> PathBuf {
        self.0.clone()
    }
}

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use crate::v2::test::test::init_log;
    use crate::v2::types::{Adb, AddressType, Client};

    static DEVICE_IP: &'static str = "192.168.1.34:5555";

    #[test]
    fn test_adb() {
        let _adb = Adb::new().expect("failed to find adb command in you PATH");
    }

    #[test]
    fn test_debug_display() {
        let w = which::which("adb").expect("failed to find adb command in you PATH");
        let adb = Adb::new().expect("failed to find adb command in you PATH");

        assert_eq!(w.to_str(), adb.as_ref().to_str());
        assert_eq!(format!("{:?}", w.to_str()), adb.to_string());
        assert_eq!(format!("{w:#?}"), format!("{adb:#?}"));

        assert_eq!(w, adb.as_ref());
        assert_eq!(w, adb.as_os_str());

        let path: PathBuf = adb.into();
        assert_eq!(w, path);
    }

    #[test]
    fn test_exec() {
        init_log();
        let adb = Adb::new().expect("failed to find adb");
        let addr = AddressType::try_from(DEVICE_IP).unwrap();
        let result = adb.exec(addr, vec!["get-state"], None, None, true).unwrap();
        println!("result: {result:?}");

        let addr = AddressType::Transport(4);
        let result = adb.exec(addr, vec!["get-state"], None, None, true).unwrap();
        println!("result: {result:?}");
    }

    #[test]
    fn test_mdns_check() {
        init_log();
        let adb = Adb::new().expect("failed to find adb");
        let _ = adb.mdns_check(true).expect("mdns unavailable");
    }

    #[test]
    fn test_list_devices() {
        init_log();
        let adb = Adb::new().expect("failed to find adb");
        let devices = adb.list_devices(true).expect("failed to list devices");
        let devices_count = devices.len();
        println!("devices attached: {devices:#?}");

        let clients: Vec<Client> = devices.into_iter().map(|device| device.try_into().expect("failed to convert AdbDevice into Client")).collect();
        assert_eq!(devices_count, clients.len());
    }
}