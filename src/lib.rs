#[macro_use]
pub mod util;

pub mod key;

pub mod pfs0;

pub mod romfs;

pub mod nca;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::new_shared;
    use std::{
        fs::{read_dir, File},
        io::Write,
    };

    #[test]
    fn pfs0_test() {
        println!("PFS0 test...");

        let pfs0_reader = new_shared(File::open("test.nsp").unwrap());
        let mut pfs0 = pfs0::PFS0::new(pfs0_reader).unwrap();

        let files = pfs0.list_files().unwrap();
        println!("Files: {:?}", pfs0.list_files().unwrap());

        for (idx, file) in files.iter().enumerate() {
            println!("Saving '{}'...", file);

            let size = pfs0.get_file_size(idx).unwrap();
            let mut file_buf = vec![0u8; size];
            pfs0.read_file(idx, 0, &mut file_buf).unwrap();

            let mut out_file = File::create(file).unwrap();
            out_file.write_all(&file_buf).unwrap();

            println!("Saved!");
        }
    }

    #[test]
    fn romfs_test() {
        println!("RomFs test...");

        let romfs_reader = new_shared(File::open("romfs.bin").unwrap());

        let mut romfs = romfs::RomFs::new(romfs_reader).unwrap();

        fn log_file(path: &str, romfs: &mut romfs::RomFs) {
            let path_str = String::from(path);
            let exists = romfs.exists_file(path_str.clone());
            if exists {
                let file_size = romfs.get_file_size(path_str.clone()).unwrap();
                println!("Found file '{}'! Size: {}", path, file_size);

                if file_size > 0 {
                    println!("Reading file...");
                    let mut file_data = vec![0u8; file_size];
                    romfs.read_file(path_str, 0, &mut file_data).unwrap();
                    println!("Read data str: {}", String::from_utf8(file_data).unwrap());
                }
            } else {
                println!("File '{}' not found...", path);
            }
        }

        log_file("a.txt", &mut romfs);
        log_file("qwe/b.txt", &mut romfs);
        log_file("qwe2/a.txt", &mut romfs);

        log_file("qwe/a.txt", &mut romfs);
        log_file("b.txt", &mut romfs);
        log_file("qwe2/b.txt", &mut romfs);
    }

    #[test]
    fn nca_test() {
        println!("NCA test...");

        let keyset = key::Keyset::from(File::open("prod.keys").unwrap()).unwrap();

        let nca_reader = new_shared(File::open("test.nca").unwrap());
        let mut nca = nca::NCA::new(nca_reader, &keyset, None).unwrap();

        for i in 0..nca.get_filesystem_count() {
            if let Ok(mut pfs0) = nca.open_pfs0_filesystem(i) {
                let files = pfs0.list_files().unwrap();

                for file_name in files {
                    println!("Saving file '{}' from NCA PFS0 section...", file_name);
                    let file_size = pfs0.get_file_size(i).unwrap();
                    let mut file_buf = vec![0u8; file_size];
                    pfs0.read_file(i, 0, &mut file_buf).unwrap();

                    let mut out_file = File::create(format!("pfs0-{}", file_name)).unwrap();
                    out_file.write_all(&file_buf).unwrap();
                    println!("Saved!");
                }
            } else if let Ok(mut romfs) = nca.open_romfs_filesystem(i) {
                let empty_file = String::from("AtLeastOneFile");

                let exists_file = romfs.exists_file(empty_file.clone());
                assert!(exists_file);
                println!("Exists empty file!");

                let file_size = romfs.get_file_size(empty_file).unwrap();
                assert_eq!(file_size, 0);
                println!("The file is empty as expected!");
            }
        }
    }

    #[test]
    fn test_read_system_version() {
        println!("NCA test...");

        let keyset = key::Keyset::from(File::open("prod.keys").unwrap()).unwrap();

        for dir_entry in read_dir("registered").unwrap().flatten() {
            println!("Reading system NCA: {:?}", dir_entry.path());

            let nca_reader = new_shared(File::open(dir_entry.path()).unwrap());
            if let Ok(mut nca) = nca::NCA::new(nca_reader, &keyset, None) {
                println!(" - Program ID: {:#018X}", nca.header.program_id);

                if nca.header.program_id == 0x0100000000000809
                    && nca.header.cnt_type == nca::ContentType::Data
                {
                    if let Ok(mut romfs) = nca.open_romfs_filesystem(0) {
                        let system_version_file = String::from("file");
                        if romfs.exists_file(system_version_file.clone()) {
                            println!("SystemVersion found! NCA: {:?}", dir_entry.path());

                            if romfs.exists_file(String::from("digest")) {
                                println!("Also has digest file! Must be 5.0.0+");
                            }

                            let mut system_version_str: [u8; 0x80] = [0; 0x80];
                            romfs
                                .read_file(system_version_file, 0x80, &mut system_version_str)
                                .unwrap();
                            println!(
                                "System version: {}",
                                String::from_utf8(system_version_str.to_vec()).unwrap()
                            );
                            println!("Done!");
                            break;
                        }
                    }
                }
            } else {
                panic!("NCA failed...");
            }
        }
    }

    #[test]
    fn test_browserdll_romfs_iter() {
        println!("NCA test...");

        let keyset = key::Keyset::from(File::open("prod.keys").unwrap()).unwrap();

        for dir_entry in read_dir("registered").unwrap().flatten() {
            println!("Reading system NCA: {:?}", dir_entry.path());

            let nca_reader = new_shared(File::open(dir_entry.path()).unwrap());
            if let Ok(mut nca) = nca::NCA::new(nca_reader, &keyset, None) {
                println!(" - Program ID: {:#018X}", nca.header.program_id);

                if nca.header.program_id == 0x0100000000000803
                    && nca.header.cnt_type == nca::ContentType::Data
                {
                    if let Ok(mut romfs) = nca.open_romfs_filesystem(0) {
                        let base_path = String::from("lyt");
                        println!("Listing BrowserDll stuff at rom:/{}...", base_path);

                        let mut dir_iter = romfs.open_dir_iterator(base_path.clone()).unwrap();
                        loop {
                            if let Ok(dir_name) = dir_iter.next_dir() {
                                println!(" - [D] rom:/{}/{}", base_path, dir_name);
                            } else if let Ok((file_name, _file_size)) = dir_iter.next_file() {
                                println!(" - [F] rom:/{}/{}", base_path, file_name);
                            } else {
                                println!("EOF!");
                                break;
                            }
                        }

                        println!("Done!");
                        break;
                    }
                }
            } else {
                panic!("NCA failed...");
            }
        }
    }
}
