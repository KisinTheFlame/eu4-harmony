//! Only include Windows minidumps whose embedded process identity can be
//! matched to the core's logs for this user folder. File names are only a
//! preliminary filter: a PID can be reused across runs and user folders.
use super::Source;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead as _, BufReader, Read, Seek, SeekFrom};
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::Path;
use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_DELETE, FILE_SHARE_READ};

/// The minidump stores process creation in whole Unix seconds, whereas the
/// journal uses the process's original Windows FILETIME.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Process {
    pid: u32,
    created: u32,
}

fn run_process(run: &str) -> Option<Process> {
    const UNIX_EPOCH_FILETIME: u64 = 116_444_736_000_000_000;
    let (pid, created) = run.split_once('-')?;
    let pid = pid.parse::<u32>().ok().filter(|&id| id != 0)?;
    let created = u64::from_str_radix(created, 16).ok()?.checked_sub(UNIX_EPOCH_FILETIME)? / 10_000_000;
    Some(Process { pid, created: created.try_into().ok()? })
}

fn log_run(file: File) -> Option<String> {
    let mut header = String::new();
    BufReader::new(file).take(512).read_line(&mut header).ok()?;
    if !eu4_harmony_logging::is_header(&header) {
        return None;
    }
    let (source, rest) = header.split_once(" source=")?.1.split_once(" run=")?;
    if !matches!(source, eu4_harmony_logging::CORE | eu4_harmony_logging::CORE_EMERGENCY) {
        return None;
    }
    Some(rest.split_ascii_whitespace().next()?.to_owned())
}

pub(super) fn find(root: &Path, configured: Option<&Path>, sources: &[Source]) -> (Vec<Source>, usize) {
    let mut runs = HashMap::new();
    for source in sources.iter().filter(|source| source.name.starts_with("harmony-logs/core")) {
        if let Some(run) = File::open(&source.path).ok().and_then(log_run) {
            if let Some(process) = run_process(&run) {
                // Reject ambiguity if two precise start times round to the
                // same second for the same PID.
                let entry = runs.entry(process).or_insert_with(|| Some(run.clone()));
                if entry.as_ref() != Some(&run) {
                    *entry = None;
                }
            }
        }
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut skipped = 0;
    let mut folders = vec![root.join("eu4-harmony"), root.join("eu4-harmony-wer"), root.to_owned()];
    if let Some(path) = configured.filter(|p| !folders.iter().any(|folder| folder == p)) {
        folders.push(path.to_owned());
    }
    for folder in folders {
        let entries = match fs::read_dir(folder) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        for entry in entries {
            let Ok(entry) = entry else {
                skipped += 1;
                continue;
            };
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            let captured = eu4_harmony_folders::captured_dump_id(&name);
            let pid = captured.map(|(pid, _)| pid).or_else(|| name.strip_prefix("eu4.exe.")?.strip_suffix(".dmp")?.parse().ok());
            let Some(pid) = pid else {
                continue;
            };
            let matched = (|| {
                if !entry.file_type().ok()?.is_file() {
                    return None;
                }
                // Refuse a file that still has a writer. Hold the handle
                // through compression even if Windows renames the file.
                let path = entry.path();
                let mut file = OpenOptions::new().read(true).share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE).open(&path).ok()?;
                let process = dump_process(&mut file).ok()?;
                if process.pid != pid {
                    return None;
                }
                let run = runs.get(&process)?.as_ref()?;
                if captured.is_some_and(|(_, created)| *run != eu4_harmony_logging::process_run_id(pid, created)) {
                    return None;
                }
                file.rewind().ok()?;
                Some((run.clone(), Source { name: format!("windows-dumps/{run}/minidump.dmp"), path, opened: Some(file) }))
            })();
            if let Some((run, source)) = matched {
                if seen.insert(run) {
                    out.push(source);
                }
            } else {
                skipped += 1;
            }
        }
    }
    out.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    (out, skipped)
}

/// Read the documented minidump directory and the common MINIDUMP_MISC_INFO
/// fields. The exception stream must also carry saved registers.
fn dump_process(file: &mut File) -> io::Result<Process> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "Windows minidump misses process identity or exception context");
    let length = file.metadata()?.len();
    let header = words::<8>(file)?;
    let [signature, _, streams, directory, ..] = header;
    if signature.to_le_bytes() != *b"MDMP" || streams > 4096 || u64::from(directory) + u64::from(streams) * 12 > length {
        return Err(invalid());
    }
    file.seek(SeekFrom::Start(u64::from(directory)))?;
    let (mut misc, mut exception) = (None, None);
    for _ in 0..streams {
        let [kind, size, offset] = words::<3>(file)?;
        if u64::from(offset) + u64::from(size) <= length {
            match kind {
                6 if size >= 168 => exception = Some(u64::from(offset)),
                15 if size >= 24 => misc = Some((u64::from(offset), size)),
                _ => (),
            }
        }
    }
    file.seek(SeekFrom::Start(exception.ok_or_else(invalid)? + 160))?;
    let [size, offset] = words::<2>(file)?;
    if size == 0 || offset == 0 || u64::from(offset) + u64::from(size) > length {
        return Err(invalid());
    }
    let (offset, stream_size) = misc.ok_or_else(invalid)?;
    file.seek(SeekFrom::Start(offset))?;
    let [size, flags, pid, created, ..] = words::<6>(file)?;
    if !(24..=stream_size).contains(&size) || flags & 3 != 3 || pid == 0 || created == 0 {
        return Err(invalid());
    }
    Ok(Process { pid, created })
}

fn words<const N: usize>(file: &mut File) -> io::Result<[u32; N]> {
    let mut words = [0; N];
    for word in &mut words {
        let mut bytes = [0; 4];
        file.read_exact(&mut bytes)?;
        *word = u32::from_le_bytes(bytes);
    }
    Ok(words)
}
