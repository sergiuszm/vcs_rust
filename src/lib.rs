use sha2::{Digest, Sha256};
use std::env;
use std::fmt::Display;
use std::fs;
use std::io::{self, prelude::*, BufReader, BufWriter};
use std::ops::Deref;
use std::path::{self, PathBuf};
use std::string::FromUtf8Error;

static VCS_DIR: &str = "vcs";
static COMMIT_DIR: &str = "commit";
static COMMIT_FILE: &str = "commit.txt";
static CONFIG_FILE: &str = "config.txt";
static INDEX_FILE: &str = "index.txt";
static LOG_FILE: &str = "log.txt";

static HELP: &str = "Usage:
  vcs [options] [arg]

OPTIONS
  -c, --config       get and set a username
  -a, --add          add a file to the index
  -l, --log          show commit logs
  -m, --commit       save changes
  -t, --checkout     restore a file";

static NOT_CONFIGURED: &str = "Please, tell me who you are. 
  Use -c | --config first!";

#[derive(Debug)]
pub enum VcsError {
    Usage(&'static str),
    IoError(io::Error),
    WrongInput(String),
    InternalError(FromUtf8Error),
    NotConfiguredError(&'static str),
}

impl Display for VcsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VcsError::Usage(usage_error) => write!(f, "{}", usage_error),
            VcsError::IoError(io_error) => write!(f, "{}", io_error),
            VcsError::WrongInput(wrong_input) => write!(f, "{}", wrong_input),
            VcsError::InternalError(internal_error) => write!(f, "{}", internal_error),
            VcsError::NotConfiguredError(not_configured_error) => {
                write!(f, "{}", not_configured_error)
            }
        }
    }
}

impl std::error::Error for VcsError {}

impl From<io::Error> for VcsError {
    fn from(err: io::Error) -> Self {
        VcsError::IoError(err)
    }
}

impl From<&'static str> for VcsError {
    fn from(err: &'static str) -> Self {
        VcsError::Usage(err)
    }
}

impl From<FromUtf8Error> for VcsError {
    fn from(err: FromUtf8Error) -> Self {
        VcsError::InternalError(err)
    }
}

pub struct Cmd {
    name: String,
    arg: Option<String>,
    // path: PathBuf
}

impl Cmd {
    pub fn new() -> Result<Cmd, VcsError> {
        let mut args = env::args();
        args.next(); // application name

        if args.len() < 1 || args.len() > 2 {
            return Err(VcsError::Usage(HELP));
        }

        Ok(Cmd {
            name: args.next().unwrap(),
            arg: args.next(),
            // path: env::current_dir()?
        })
    }

    // fn push<P: AsRef<Path>>(&mut self, path: P) {
    //     self.path.push(path);
    // }

    // fn pop(&mut self) -> bool {

    // }

    pub fn execute(&self) -> Result<(), VcsError> {
        let is_configured = self.get_author().is_ok();

        match (self.name.as_str(), &self.arg, is_configured) {
            ("-c" | "--config", _, _) => self.do_config()?,
            ("-a" | "--add", Some(_), false) => {
                return Err(VcsError::NotConfiguredError(NOT_CONFIGURED))
            }
            ("-a" | "--add", _, true) => self.do_add()?,
            ("-l" | "--log", _, _) => self.do_log()?,
            ("-m" | "--commit", Some(_), false) => {
                return Err(VcsError::NotConfiguredError(NOT_CONFIGURED))
            }
            ("-m" | "--commit", Some(_), true) => self.do_commit()?,
            ("-m" | "--commit", None, _) => return Err(VcsError::Usage("Message was not passed.")),
            ("-t" | "--checkout", Some(_), false) => {
                return Err(VcsError::NotConfiguredError(NOT_CONFIGURED))
            }
            ("-t" | "--checkout", Some(_), true) => self.do_checkout()?,
            ("-t" | "--checkout", None, _) => {
                return Err(VcsError::Usage("Commit ID was not passed."))
            }
            _ => return Err(VcsError::Usage(HELP)),
        }

        Ok(())
    }

    fn get_author(&self) -> Result<String, VcsError> {
        let mut path = get_default_path()?;
        path.push(VCS_DIR);
        path.push(CONFIG_FILE);

        let conf_file = fs::OpenOptions::new().read(true).open(&path)?;
        let mut reader = BufReader::new(&conf_file);
        let mut buffer = String::new();

        reader.read_line(&mut buffer)?;
        if buffer.is_empty() {
            return Err(VcsError::NotConfiguredError(NOT_CONFIGURED));
        }

        Ok(buffer)
    }

    fn do_config(&self) -> Result<(), io::Error> {
        let configured_name = self.get_author().unwrap_or_default();
        let is_configured = !configured_name.is_empty();

        match (&self.arg, is_configured) {
            (Some(name), _) => {
                let mut path = get_default_path()?;
                path.push(VCS_DIR);
                path.push(CONFIG_FILE);

                let conf_file = fs::OpenOptions::new().write(true).open(path)?;
                let mut writter = BufWriter::new(&conf_file);
                let _ = writter.write(name.as_bytes())?;
                println!("The username is {name}.");
            }
            (None, true) => println!("The username is {configured_name}."),
            _ => println!("Please, tell me who you are."),
        }

        Ok(())
    }

    fn do_add(&self) -> Result<(), VcsError> {
        let mut path = get_default_path()?;

        path.push(VCS_DIR);
        path.push(INDEX_FILE);
        let index_file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .append(true)
            .open(&path)?;

        let mut path = get_default_path()?;
        let file_name: &str = self.arg.as_deref().unwrap_or("");

        if !file_name.is_empty() {
            path.push(file_name);
            if !path::Path::new(&path).exists() {
                return Err(VcsError::WrongInput(format!(
                    "File '{}' does not exist!",
                    file_name
                )));
            }
            path.pop();
        }

        let mut is_indexed = false;
        let mut total_bytes_read = 0;
        let mut reader = BufReader::new(&index_file);
        let mut content = String::new();

        loop {
            let bytes_read = reader.read_line(&mut content)?;
            total_bytes_read += bytes_read;

            if bytes_read == 0 {
                break;
            }

            if !file_name.is_empty() && content.contains(file_name) {
                is_indexed = true;
            }
        }

        match (file_name.is_empty(), is_indexed, total_bytes_read > 0) {
            (false, true, _) => println!("The file {file_name} is already tracked!"),
            (false, false, _) => {
                let mut writter = BufWriter::new(&index_file);
                let _ = writter.write(file_name.as_bytes())?;
                let _ = writter.write("\n".as_bytes())?;
                println!("The file {file_name} is tracked!");
            }
            (true, _, false) => println!("Add a file to the index."),
            (true, _, true) => println!("{}", content.trim()),
        }

        Ok(())
    }

    fn do_commit(&self) -> Result<(), VcsError> {
        let mut tracked_files = self.get_lines_from_file(INDEX_FILE)?;

        if tracked_files.is_empty() {
            return Err(VcsError::Usage("You need to add files first."));
        }

        tracked_files.sort();
        let mut hasher = Sha256::new();

        for file_name in &tracked_files {
            self.update_hash_with_context(&mut hasher, file_name)?;
        }

        let hash = format!("{:x}", hasher.finalize());
        let commits = self.get_lines_from_file(COMMIT_FILE)?;

        if !commits.is_empty() && hash.eq(commits.first().unwrap()) {
            return Err(VcsError::Usage("Nothing changed since the last commit."));
        }

        let mut updated_commits = vec![hash];
        updated_commits.extend(commits);

        let mut path = get_default_path()?;
        path.push(VCS_DIR);
        path.push(COMMIT_FILE);

        let mut commit_file = fs::OpenOptions::new().write(true).open(&path)?;
        commit_file.write_all(updated_commits.join("\n").as_bytes())?;
        path.pop(); // COMMIT_FILE

        let hash = updated_commits.first().unwrap();
        path.push(COMMIT_DIR);
        path.push(hash);
        fs::create_dir_all(&path)?;

        let mut src_path = get_default_path()?;
        let mut dst_path = path;

        self.copy_files(&mut src_path, &mut dst_path, tracked_files)?;

        let log = self.get_lines_from_file(LOG_FILE)?;
        let mut new_log = vec![
            format!("commit {hash}"),
            format!("Author: {}", self.get_author()?),
            format!("{}\n", self.arg.as_deref().unwrap()),
        ];

        new_log.extend(log);

        let mut path = src_path;
        path.push(VCS_DIR);
        path.push(LOG_FILE);
        let mut log_file = fs::OpenOptions::new().write(true).open(&path)?;
        log_file.write_all(new_log.join("\n").as_bytes())?;
        path.pop(); // LOG_FILE

        path.push(INDEX_FILE);
        fs::File::create(&path)?;

        println!("A commit with ID: {} was succesfully created.", hash);

        Ok(())
    }

    fn do_log(&self) -> Result<(), VcsError> {
        let log = self.get_lines_from_file(LOG_FILE)?;

        for line in log {
            println!("{line}");
        }

        Ok(())
    }

    fn do_checkout(&self) -> Result<(), VcsError> {
        let commits = self.get_lines_from_file(COMMIT_FILE)?;

        if commits.is_empty() {
            return Err(VcsError::Usage("Repository does not have any commits."));
        }

        let commit = commits.iter().find(|&x| x.eq(self.arg.as_deref().unwrap()));
        if commit.is_none() {
            return Err(VcsError::WrongInput(format!(
                "Commit with ID: {} does not exist.",
                self.arg.as_deref().unwrap()
            )));
        }

        let commit_id = commit.unwrap();
        let mut path = get_default_path()?;
        path.push(VCS_DIR);
        path.push(COMMIT_DIR);
        path.push(commit_id);

        let mut src_path = path;
        let mut dst_path = get_default_path()?;

        let paths = fs::read_dir(&src_path)?;
        let file_names: Vec<String> = paths
            .map(|x| {
                x.unwrap_or_else(|_| {
                    panic!(
                        "Error when reading directory: {}",
                        src_path.to_string_lossy()
                    )
                })
            })
            .map(|x| {
                x.file_name()
                    .into_string()
                    .expect("Can't convert OsString into String")
            })
            .collect();

        self.copy_files(&mut src_path, &mut dst_path, file_names)?;

        println!("Commit with ID: {} checked out succesfully.", commit_id);

        Ok(())
    }

    fn copy_files(
        &self,
        src_path: &mut PathBuf,
        dst_path: &mut PathBuf,
        file_names: Vec<String>,
    ) -> Result<(), VcsError> {
        for file_name in file_names {
            src_path.push(&file_name);
            dst_path.push(file_name);
            fs::copy(&src_path, &dst_path)?;
            src_path.pop();
            dst_path.pop();
        }

        Ok(())
    }

    fn get_lines_from_file(&self, file_name: &'static str) -> Result<Vec<String>, VcsError> {
        let mut path = get_default_path()?;
        path.push(VCS_DIR);
        path.push(file_name);

        let file = fs::OpenOptions::new().read(true).open(&path)?;
        let reader = BufReader::new(file);
        let error_msg = format!("Could not parse {}", file_name.deref());
        let file_lines: Vec<String> = reader.lines().map(|l| l.expect(&error_msg)).collect();

        Ok(file_lines)
    }

    fn update_hash_with_context(
        &self,
        hasher: &mut Sha256,
        file_name: &String,
    ) -> Result<(), io::Error> {
        let mut path = get_default_path()?;

        path.push(file_name);
        let file = fs::OpenOptions::new().read(true).open(&path)?;

        let reader = BufReader::new(&file);
        let context: Vec<String> = reader
            .lines()
            .map(|l| l.expect("Could not parse the line."))
            .collect();

        hasher.update(file_name);
        for line in context {
            hasher.update(line);
        }

        Ok(())
    }
}

fn get_default_path() -> Result<PathBuf, io::Error> {
    let path = env::current_dir()?;

    Ok(path)
}

pub fn setup_dir_structure() -> Result<(), io::Error> {
    let mut path = env::current_dir()?;
    path.push(VCS_DIR);
    fs::create_dir_all(&path)?;

    let files = vec![CONFIG_FILE, INDEX_FILE, LOG_FILE, COMMIT_FILE];

    for file in files.iter() {
        path.push(file);
        if !path::Path::new(&path).exists() {
            fs::File::create(&path)?;
        }
        path.pop();
    }

    path.push(COMMIT_DIR);
    fs::create_dir_all(&path)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::panic;

    struct Setup {
        default_path: PathBuf,
    }

    impl Setup {
        fn new() -> Setup {
            let path = get_default_path().unwrap();
            fs::create_dir_all(&path).unwrap();

            Setup { default_path: path }
        }

        fn create_env_for(self, cmd: &'static str) {}

        // fn teardown(self) {
        //     println!("I'm melting! Meeeelllllttttinnnng!");
        //     fs::remove_dir_all(&self.default_path).unwrap();
        // }
    }

    impl Drop for Setup {
        fn drop(&mut self) {
            println!("I'm melting! Meeeelllllttttinnnng!");
            fs::remove_dir_all(&self.default_path).unwrap();
        }
    }

    fn get_default_path() -> Result<PathBuf, io::Error> {
        let mut path = env::temp_dir();
        path.push("vcs_test_dir");

        Ok(path)
    }

    fn run_test<T>(cmd: &'static str, test: T)
    where
        T: FnOnce() + panic::UnwindSafe,
    {
        let setup = Setup::new();
        setup.create_env_for(cmd);

        let result = panic::catch_unwind(|| test());

        // setup.teardown();

        assert!(result.is_ok())
    }

    #[test]
    #[serial]
    fn test_default_path() {
        run_test("", || {
            let path = get_default_path().unwrap();

            assert!(path.to_str().unwrap().contains("vcs_test_dir"));
        })
    }

    #[test]
    #[serial]
    fn test_panic() {
        run_test("", || {
            panic!("Just for a test");
        })
    }
}
