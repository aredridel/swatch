use chrono::DateTime;
use clap::{crate_version, Arg, ArgAction, Command};
use fuser::{
    FileAttr, FileType, Filesystem, MountOption, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry,
    Request,
};
use libc::ENOENT;
use openat::{Dir, Metadata};
use std::ffi::OsStr;
use std::io::ErrorKind;
use std::time::Duration;

const TTL: Duration = Duration::from_secs(1); // 1 second

struct SwatchFS {
    root: Dir,
}

fn meta_into_file_attr(m: &Metadata) -> FileAttr {
    let s = m.stat();
    let typ = s.st_mode & libc::S_IFMT;
    FileAttr {
        atime: DateTime::from_timestamp(s.st_atime, 0).unwrap().into(),
        mtime: DateTime::from_timestamp(s.st_mtime, 0).unwrap().into(),
        ctime: DateTime::from_timestamp(s.st_ctime, 0).unwrap().into(),
        crtime: DateTime::from_timestamp(s.st_birthtime, 0).unwrap().into(),
        ino: s.st_ino,
        blksize: s.st_blksize as u32,
        size: s.st_size as u64,
        blocks: s.st_blocks as u64,
        flags: s.st_flags,
        gid: s.st_gid,
        uid: s.st_uid,
        nlink: s.st_nlink as u32,
        perm: s.st_mode & !libc::S_IFMT,
        rdev: s.st_rdev as u32,
        kind: match typ {
            libc::S_IFREG => FileType::RegularFile,
            libc::S_IFDIR => FileType::Directory,
            libc::S_IFLNK => FileType::Symlink,
            libc::S_IFBLK => FileType::BlockDevice,
            libc::S_IFCHR => FileType::CharDevice,
            libc::S_IFIFO => FileType::NamedPipe,
            libc::S_IFSOCK => FileType::Socket,
            _ => panic!("unknown file type {:?}", typ),
        },
    }
}

impl Filesystem for SwatchFS {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let meta = self.root.metadata(name);
        if let Err(x) = meta {
            if x.kind() == ErrorKind::NotFound {
                reply.error(ENOENT)
            }
        } else if parent == 1 && name.to_str() == Some("hello.txt") {
            reply.entry(&TTL, &meta_into_file_attr(&meta.unwrap()), 0);
        } else {
            reply.error(ENOENT);
        }
    }

    fn getattr(&mut self, _req: &Request, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        println!("{:?} {:?}", _req, ino);
        match ino {
            1 => reply.attr(
                &TTL,
                &meta_into_file_attr(&self.root.self_metadata().unwrap()),
            ),
            //2 => reply.attr(&TTL, &HELLO_TXT_ATTR),
            _ => reply.error(ENOENT),
        }
    }

    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        _size: u32,
        _flags: i32,
        _lock: Option<u64>,
        reply: ReplyData,
    ) {
        if ino == 2 {
            //reply.data(&HELLO_TXT_CONTENT.as_bytes()[offset as usize..]);
        } else {
            reply.error(ENOENT);
        }
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        if ino != 1 {
            reply.error(ENOENT);
            return;
        }

        let entries = vec![
            (1, FileType::Directory, "."),
            (1, FileType::Directory, ".."),
            (2, FileType::RegularFile, "hello.txt"),
        ];

        for (i, entry) in entries.into_iter().enumerate().skip(offset as usize) {
            // i + 1 means the index of the next entry
            if reply.add(entry.0, (i + 1) as i64, entry.1, entry.2) {
                break;
            }
        }
        reply.ok();
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Command::new("hello")
        .version(crate_version!())
        .author("Christopher Berner")
        .arg(
            Arg::new("SOURCE")
                .required(true)
                .index(1)
                .help("Directory to monitor"),
        )
        .arg(
            Arg::new("MOUNT_POINT")
                .required(true)
                .index(2)
                .help("Act as a client, and mount FUSE at given path"),
        )
        .arg(
            Arg::new("allow-root")
                .long("allow-root")
                .action(ArgAction::SetTrue)
                .help("Allow root user to access filesystem"),
        )
        .arg(
            Arg::new("command")
                .required(true)
                .num_args(1..)
                .index(3)
                .last(true)
                .help("The command to execute"),
        );
    let matches = args.get_matches();

    env_logger::init();
    let mountpoint = matches.get_one::<String>("MOUNT_POINT").unwrap();
    let sourcepoint = matches.get_one::<String>("SOURCE").unwrap();
    let options = vec![
        MountOption::RO,
        MountOption::FSName("hello".to_string()),
        MountOption::AllowOther,
        MountOption::AutoUnmount,
    ];
    let root = Dir::open(sourcepoint)?;
    let mounted = fuser::spawn_mount2(SwatchFS { root }, mountpoint, &options).unwrap();

    {
        use std::process::Command;
        let mut p = matches.get_many::<String>("command").unwrap();
        let mut cmd = Command::new(p.next().unwrap());
        cmd.args(p);
        cmd.spawn().unwrap();
    }

    mounted.join();

    Ok(())
}
