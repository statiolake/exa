//! Files, and methods and fields to access their metadata.

use std::fs;
use std::io::Error as IOError;
use std::io::Result as IOResult;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};

use fs::dir::Dir;
use fs::fields as f;

/// A **File** is a wrapper around one of Rust's Path objects, along with
/// associated data about the file.
///
/// Each file is definitely going to have its filename displayed at least
/// once, have its file extension extracted at least once, and have its metadata
/// information queried at least once, so it makes sense to do all this at the
/// start and hold on to all the information.
pub struct File<'dir> {
    /// The filename portion of this file’s path, including the extension.
    ///
    /// This is used to compare against certain filenames (such as checking if
    /// it’s “Makefile” or something) and to highlight only the filename in
    /// colour when displaying the path.
    pub name: String,

    /// The file’s name’s extension, if present, extracted from the name.
    ///
    /// This is queried many times over, so it’s worth caching it.
    pub ext: Option<String>,

    /// The path that begat this file.
    ///
    /// Even though the file’s name is extracted, the path needs to be kept
    /// around, as certain operations involve looking up the file’s absolute
    /// location (such as searching for compiled files) or using its original
    /// path (following a symlink).
    pub path: PathBuf,

    /// A cached `metadata` (`stat`) call for this file.
    ///
    /// This too is queried multiple times, and is *not* cached by the OS, as
    /// it could easily change between invocations — but exa is so short-lived
    /// it's better to just cache it.
    pub metadata: fs::Metadata,

    /// A reference to the directory that contains this file, if any.
    ///
    /// Filenames that get passed in on the command-line directly will have no
    /// parent directory reference — although they technically have one on the
    /// filesystem, we’ll never need to look at it, so it’ll be `None`.
    /// However, *directories* that get passed in will produce files that
    /// contain a reference to it, which is used in certain operations (such
    /// as looking up compiled files).
    pub parent_dir: Option<&'dir Dir>,
}

impl<'dir> File<'dir> {
    pub fn new<PD, FN>(path: PathBuf, parent_dir: PD, filename: FN) -> IOResult<File<'dir>>
    where
        PD: Into<Option<&'dir Dir>>,
        FN: Into<Option<String>>,
    {
        let parent_dir = parent_dir.into();
        let name = filename.into().unwrap_or_else(|| File::filename(&path));
        let ext = File::ext(&path);

        debug!("Statting file {:?}", &path);
        let metadata = fs::symlink_metadata(&path)?;

        Ok(File {
            path,
            parent_dir,
            metadata,
            ext,
            name,
        })
    }

    /// A file’s name is derived from its string. This needs to handle directories
    /// such as `/` or `..`, which have no `file_name` component. So instead, just
    /// use the last component as the name.
    pub fn filename(path: &Path) -> String {
        if let Some(back) = path.components().next_back() {
            back.as_os_str().to_string_lossy().to_string()
        } else {
            // use the path as fallback
            error!("Path {:?} has no last component", path);
            path.display().to_string()
        }
    }

    /// Extract an extension from a file path, if one is present, in lowercase.
    ///
    /// The extension is the series of characters after the last dot. This
    /// deliberately counts dotfiles, so the “.git” folder has the extension “git”.
    ///
    /// ASCII lowercasing is used because these extensions are only compared
    /// against a pre-compiled list of extensions which are known to only exist
    /// within ASCII, so it’s alright.
    fn ext(path: &Path) -> Option<String> {
        let name = path.file_name().map(|f| f.to_string_lossy().to_string())?;

        name.rfind('.').map(|p| name[p + 1..].to_ascii_lowercase())
    }

    /// Whether this file is a directory on the filesystem.
    pub fn is_directory(&self) -> bool {
        self.metadata.is_dir()
    }

    /// Whether this file is a directory, or a symlink pointing to a directory.
    pub fn points_to_directory(&self) -> bool {
        if self.is_directory() {
            return true;
        }

        if self.is_link() {
            let target = self.link_target();
            if let FileTarget::Ok(target) = target {
                return target.points_to_directory();
            }
        }

        false
    }

    /// If this file is a directory on the filesystem, then clone its
    /// `PathBuf` for use in one of our own `Dir` values, and read a list of
    /// its contents.
    ///
    /// Returns an IO error upon failure, but this shouldn’t be used to check
    /// if a `File` is a directory or not! For that, just use `is_directory()`.
    pub fn to_dir(&self) -> IOResult<Dir> {
        Dir::read_dir(self.path.clone())
    }

    /// Whether this file is a regular file on the filesystem — that is, not a
    /// directory, a link, or anything else treated specially.
    pub fn is_file(&self) -> bool {
        self.metadata.is_file()
    }

    /// Whether this file is both a regular file *and* executable for the
    /// current user. An executable file has a different purpose from an
    /// executable directory, so they should be highlighted differently.
    pub fn is_executable_file(&self) -> bool {
        self.is_file() && self.ext.as_ref().filter(|&x| x == "exe").is_some()
    }

    /// Whether this file is a symlink on the filesystem.
    pub fn is_link(&self) -> bool {
        self.metadata.file_type().is_symlink()
    }

    /// Whether this file is a named pipe on the filesystem.
    pub fn is_pipe(&self) -> bool {
        // TODO: implement it using WinAPI
        false
    }

    /// Whether this file is a char device on the filesystem.
    pub fn is_char_device(&self) -> bool {
        // TODO: what is char device?
        false
    }

    /// Whether this file is a block device on the filesystem.
    pub fn is_block_device(&self) -> bool {
        // TODO: implement it using WinAPI
        false
    }

    /// Whether this file is a socket on the filesystem.
    pub fn is_socket(&self) -> bool {
        // TODO: implement it using WinAPI
        false
    }

    /// Re-prefixes the path pointed to by this file, if it’s a symlink, to
    /// make it an absolute path that can be accessed from whichever
    /// directory exa is being run from.
    fn reorient_target_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else if let Some(dir) = self.parent_dir {
            dir.join(&*path)
        } else if let Some(parent) = self.path.parent() {
            parent.join(&*path)
        } else {
            self.path.join(&*path)
        }
    }

    /// Again assuming this file is a symlink, follows that link and returns
    /// the result of following it.
    ///
    /// For a working symlink that the user is allowed to follow,
    /// this will be the `File` object at the other end, which can then have
    /// its name, colour, and other details read.
    ///
    /// For a broken symlink, returns where the file *would* be, if it
    /// existed. If this file cannot be read at all, returns the error that
    /// we got when we tried to read it.
    pub fn link_target(&self) -> FileTarget<'dir> {
        // We need to be careful to treat the path actually pointed to by
        // this file — which could be absolute or relative — to the path
        // we actually look up and turn into a `File` — which needs to be
        // absolute to be accessible from any directory.
        debug!("Reading link {:?}", &self.path);
        let path = match fs::read_link(&self.path) {
            Ok(p) => p,
            Err(e) => return FileTarget::Err(e),
        };

        let absolute_path = self.reorient_target_path(&path);

        // Use plain `metadata` instead of `symlink_metadata` - we *want* to
        // follow links.
        match fs::metadata(&absolute_path) {
            Ok(metadata) => {
                let ext = File::ext(&path);
                let name = File::filename(&path);
                FileTarget::Ok(Box::new(File {
                    parent_dir: None,
                    path,
                    ext,
                    metadata,
                    name,
                }))
            }
            Err(e) => {
                error!("Error following link {:?}: {:#?}", &path, e);
                FileTarget::Broken(path)
            }
        }
    }

    /// This file’s number of hard links.
    ///
    /// It also reports whether this is both a regular file, and a file with
    /// multiple links. This is important, because a file with multiple links
    /// is uncommon, while you come across directories and other types
    /// with multiple links much more often. Thus, it should get highlighted
    /// more attentively.
    pub fn links(&self) -> f::Links {
        // TODO: implement it using WinAPI
        let count = 0;

        f::Links {
            count,
            multiple: self.is_file() && count > 1,
        }
    }

    /// This file's inode.
    pub fn inode(&self) -> f::Inode {
        // TODO: implement it
        f::Inode(0)
    }

    /// This file's number of filesystem blocks.
    ///
    /// (Not the size of each block, which we don't actually report on)
    pub fn blocks(&self) -> f::Blocks {
        if self.is_file() || self.is_link() {
            // TODO: implement it
            f::Blocks::Some(0)
        } else {
            f::Blocks::None
        }
    }

    /// The ID of the user that own this file.
    pub fn user(&self) -> f::User {
        // TODO: implement it
        f::User(0)
    }

    /// The ID of the group that owns this file.
    pub fn group(&self) -> f::Group {
        // TODO: implement it
        f::Group(0)
    }

    /// This file’s size, if it’s a regular file.
    ///
    /// For directories, no size is given. Although they do have a size on
    /// some filesystems, I’ve never looked at one of those numbers and gained
    /// any information from it. So it’s going to be hidden instead.
    ///
    /// Block and character devices return their device IDs, because they
    /// usually just have a file size of zero.
    pub fn size(&self) -> f::Size {
        if self.is_directory() {
            f::Size::None
        } else if self.is_char_device() || self.is_block_device() {
            // TODO: implement it
            let dev = 0;
            f::Size::DeviceIDs(f::DeviceIDs {
                major: (dev / 256) as u8,
                minor: (dev % 256) as u8,
            })
        } else {
            f::Size::Some(self.metadata.len())
        }
    }

    /// This file’s last modified timestamp.
    pub fn modified_time(&self) -> f::Time {
        // TODO: support time zone
        let (seconds, nanoseconds) = nt_to_unix_epoch(self.metadata.creation_time());
        f::Time {
            seconds,
            nanoseconds,
        }
    }

    /// This file’s created timestamp.
    pub fn created_time(&self) -> f::Time {
        // TODO: impelement it
        f::Time {
            seconds: 0,
            nanoseconds: 0,
        }
    }

    /// This file’s last accessed timestamp.
    pub fn accessed_time(&self) -> f::Time {
        // TODO: impelement it
        f::Time {
            seconds: 0,
            nanoseconds: 0,
        }
    }

    /// This file’s ‘type’.
    ///
    /// This is used a the leftmost character of the permissions column.
    /// The file type can usually be guessed from the colour of the file, but
    /// ls puts this character there.
    pub fn type_char(&self) -> f::Type {
        if self.is_file() {
            f::Type::File
        } else if self.is_directory() {
            f::Type::Directory
        } else if self.is_pipe() {
            f::Type::Pipe
        } else if self.is_link() {
            f::Type::Link
        } else if self.is_char_device() {
            f::Type::CharDevice
        } else if self.is_block_device() {
            f::Type::BlockDevice
        } else if self.is_socket() {
            f::Type::Socket
        } else {
            f::Type::Special
        }
    }

    /// This file’s permissions, with flags for each bit.
    pub fn permissions(&self) -> f::Permissions {
        // TODO: Rewrite them using WinAPI.
        f::Permissions {
            user_read: true,
            user_write: true,
            user_execute: true,

            group_read: true,
            group_write: true,
            group_execute: true,

            other_read: true,
            other_write: true,
            other_execute: true,

            sticky: false,
            setgid: false,
            setuid: false,
        }
    }

    /// Whether this file’s extension is any of the strings that get passed in.
    ///
    /// This will always return `false` if the file has no extension.
    pub fn extension_is_one_of(&self, choices: &[&str]) -> bool {
        match self.ext {
            Some(ref ext) => choices.contains(&&ext[..]),
            None => false,
        }
    }

    /// Whether this file's name, including extension, is any of the strings
    /// that get passed in.
    pub fn name_is_one_of(&self, choices: &[&str]) -> bool {
        choices.contains(&&self.name[..])
    }
}

fn nt_to_unix_epoch(nt: u64) -> (i64, i64) {
    let nt = nt as i64;
    let nanoseconds = (nt % 1000_000_0) * 100;
    let seconds = nt / 1000_000_0 - 11644473600;
    (seconds, nanoseconds)
}

impl<'a> AsRef<File<'a>> for File<'a> {
    fn as_ref(&self) -> &File<'a> {
        self
    }
}

/// The result of following a symlink.
pub enum FileTarget<'dir> {
    /// The symlink pointed at a file that exists.
    Ok(Box<File<'dir>>),

    /// The symlink pointed at a file that does not exist. Holds the path
    /// where the file would be, if it existed.
    Broken(PathBuf),

    /// There was an IO error when following the link. This can happen if the
    /// file isn’t a link to begin with, but also if, say, we don’t have
    /// permission to follow it.
    Err(IOError),
    // Err is its own variant, instead of having the whole thing be inside an
    // `IOResult`, because being unable to follow a symlink is not a serious
    // error -- we just display the error message and move on.
}

impl<'dir> FileTarget<'dir> {
    /// Whether this link doesn’t lead to a file, for whatever reason. This
    /// gets used to determine how to highlight the link in grid views.
    pub fn is_broken(&self) -> bool {
        match *self {
            FileTarget::Ok(_) => false,
            FileTarget::Broken(_) | FileTarget::Err(_) => true,
        }
    }
}

/// More readable aliases for the permission bits exposed by libc.
#[allow(trivial_numeric_casts)]
mod modes {

    // The `libc::mode_t` type’s actual type varies, but the value returned
    // from `metadata.permissions().mode()` is always `u32`.
}

#[cfg(test)]
mod ext_test {
    use super::File;
    use std::path::Path;

    #[test]
    fn extension() {
        assert_eq!(Some("dat".to_string()), File::ext(Path::new("fester.dat")))
    }

    #[test]
    fn dotfile() {
        assert_eq!(Some("vimrc".to_string()), File::ext(Path::new(".vimrc")))
    }

    #[test]
    fn no_extension() {
        assert_eq!(None, File::ext(Path::new("jarlsberg")))
    }
}

#[cfg(test)]
mod filename_test {
    use super::File;
    use std::path::Path;

    #[test]
    fn file() {
        assert_eq!("fester.dat", File::filename(Path::new("fester.dat")))
    }

    #[test]
    fn no_path() {
        assert_eq!("foo.wha", File::filename(Path::new("/var/cache/foo.wha")))
    }

    #[test]
    fn here() {
        assert_eq!(".", File::filename(Path::new(".")))
    }

    #[test]
    fn there() {
        assert_eq!("..", File::filename(Path::new("..")))
    }

    #[test]
    fn everywhere() {
        assert_eq!("..", File::filename(Path::new("./..")))
    }

    #[test]
    fn topmost() {
        assert_eq!("/", File::filename(Path::new("/")))
    }
}
