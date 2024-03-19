use std::{collections::{HashMap, HashSet}, fs::{self, File}, io, os::unix::fs::MetadataExt, path::{Path, PathBuf}, time::Instant};

use serde::Deserialize;
use walkdir::{DirEntry, WalkDir};
use zip::{write::FileOptions, ZipWriter};

#[derive(Deserialize)]
struct Config {
   root: String,
   root_name: String,
   root_compression: String,
   output: String,
   zip_limit: u64,
   zip: HashMap<String, ZipConfig>,
}

#[derive(Deserialize, Clone)]
struct ZipConfig {
    path: String,
    compression: String,
    skip: Option<bool>,
}

struct MultiZip {
    output: String,
    root: String,
    name: String,
    zip_limit: u64,
    current_path: PathBuf,
    current_size: u64,
    total_size: u64,
    total_files: u64,
    total_zip_size: u64,
    current_number: i32,
    writer: ZipWriter<File>,
    options: FileOptions,
}

fn main() -> io::Result<()> {
    let config_file = fs::read_to_string("./config.toml").expect("Failed to read config.toml file");
    let config: Config = toml::from_str(&config_file).expect("Failed to parse config.toml file");
    
    // Create output folder if missing
    fs::create_dir_all(&config.output)?;
    
    println!("-- Fancy Zip Packer --");
    println!("Root: {}", config.root);
    println!("Output: {}", config.output);
    println!("-----");

    // Remove old output files for each non-skipped zip
    let mut zip_names: Vec<String> = config.zip.clone().into_iter().filter(|info| {
        if let Some(skip) = info.1.skip {
            return !skip;
        }
        return true;
    }).map(|info| info.0).collect();
    zip_names.push(config.root_name.clone());
    for entry in fs::read_dir(&config.output)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            let name = path.file_name().map_or_else(|| String::new(), |osstr| osstr.to_string_lossy().to_string());
            for n in &zip_names {
                if name.starts_with(n) {
                    fs::remove_file(path)?;
                    break;
                }
            }
        }
    }


    for (name, info) in config.zip.clone() {
        if let Some(skip) = info.skip {
            if skip {
                println!("Skipping '{}'", &name);
                println!("");
                continue;
            }
        }
        let start = Instant::now();
        let c_method = string_to_compression_method(&info.compression);
        let mut zip = MultiZip::open(&config.root, &name, config.zip_limit, c_method, &config.output)?;
        zip_directory(&mut zip, info)?;
        zip.close()?;
        
        let duration = start.elapsed();
        let data_rate = (zip.total_size as f64 / (1024.0 * 1024.0)) / duration.as_secs_f64();
        let compression_ratio = (1.0 - (zip.total_zip_size as f64 / zip.total_size as f64)) * 100.0;
        println!("Size: {} ({} - {:.1}%) - Files - {} - Time Taken: {:.2?} - Compression Rate: {:.2?} MB/s", human_readable_bytes(zip.total_zip_size), human_readable_bytes(zip.total_size), compression_ratio, zip.total_files, duration, data_rate);
        println!("");
    }

    println!("Building '{}' Root Zip - Compression: {}", config.root_name, config.root_compression);
    let start = Instant::now();
    let c_method = string_to_compression_method(&config.root_compression);
    let mut zip = MultiZip::open(&config.root, &config.root_name, config.zip_limit, c_method, &config.output)?;
    let excluded_roots: HashSet<PathBuf> = config.zip.into_iter().map(|info| Path::new(&config.root).join(&info.1.path)).collect();
    for entry in WalkDir::new(&config.root).into_iter().filter_entry(|e| !is_excluded(e, &excluded_roots)) {
        let entry = entry?;

        if entry.file_type().is_dir() {
            continue; // We don't care about directories
        }

        zip.add_file(entry.path(), entry.metadata()?.size())?;
    }
    zip.close()?;
    let duration = start.elapsed();
    let data_rate = (zip.total_size as f64 / (1024.0 * 1024.0)) / duration.as_secs_f64();
    let compression_ratio = (1.0 - (zip.total_zip_size as f64 / zip.total_size as f64)) * 100.0;
    println!("Size: {} ({} - {:.1}%) - Files - {} - Time Taken: {:.2?} - Compression Rate: {:.2?} MB/s", human_readable_bytes(zip.total_zip_size), human_readable_bytes(zip.total_size), compression_ratio, zip.total_files, duration, data_rate);
    
    Ok(())
}

fn is_excluded(entry: &DirEntry, excluded_roots: &HashSet<PathBuf>) -> bool {
    let path = entry.path();
    excluded_roots.iter().any(|excluded_root| path.starts_with(excluded_root))
}

fn zip_directory(zip: &mut MultiZip, info: ZipConfig) ->  io::Result<()> {
    let path = Path::new(&zip.root).join(&info.path);
    println!("Building '{}' Zip - Compression: {} - Path: {:?}", &zip.name, info.compression, path);
    for entry in WalkDir::new(path) {
        let entry = entry?;

        if entry.file_type().is_dir() {
            continue; // We don't care about directories
        }

        zip.add_file(entry.path(), entry.metadata()?.size())?;
    }

    Ok(())
}

impl MultiZip {
    fn open(root: &str, name: &str, zip_limit: u64, compression: zip::CompressionMethod, output: &str) ->  io::Result<MultiZip> {
        let new_path = Path::new(output).join(format!("{}_{}.zip", name, 1));
        let file = File::create(&new_path)?;
        let writer = ZipWriter::new(file);
        Ok(MultiZip {
            output: output.to_owned(),
            root: root.to_owned(),
            name: name.to_owned(),
            current_path: new_path,
            current_number: 1,
            current_size: 0,
            total_size: 0,
            total_files: 0,
            total_zip_size: 0,
            writer,
            zip_limit,
            options: FileOptions::default().compression_method(compression),
        })
    }

    fn _new_zip(&mut self) ->  io::Result<()> {
        // Close existing writer
        self.writer.finish()?;
        self.total_size += self.current_size;
        self.total_zip_size += fs::metadata(&self.current_path)?.size();
        
        // Increment zip number and open new zip
        self.current_number += 1;
        let new_path = Path::new(&self.output).join(format!("{}_{}.zip", self.name, self.current_number));
        let file = File::create(new_path)?;
        self.writer = ZipWriter::new(file);
        self.current_size = 0;

        Ok(())
    }

    fn add_file(&mut self, path: &Path, size: u64) -> io::Result<()> {
        // Update total size
        self.current_size += size;
        self.total_files += 1;

        // Write file to archive
        self.writer.start_file(path.to_string_lossy(), self.options)?;
        let mut file = File::open(path)?;
        io::copy(&mut file, &mut self.writer)?;

        // Make a new zip if we're over the limit
        if self.current_size > self.zip_limit {
            self._new_zip()?;
        }

        Ok(())
    }

    fn close(&mut self) -> io::Result<()> {
        self.writer.finish()?;
        self.total_size += self.current_size;
        self.total_zip_size += fs::metadata(&self.current_path)?.size();
        Ok(())
    }
}

fn string_to_compression_method(s: &str) -> zip::CompressionMethod {
    match s.to_lowercase().as_str() {
        "store" => zip::CompressionMethod::STORE,
        "deflate" => zip::CompressionMethod::DEFLATE,
        "zstd" => zip::CompressionMethod::ZSTD,
        "bzip2" => zip::CompressionMethod::BZIP2,
        _ => zip::CompressionMethod::STORE
    }
}

fn human_readable_bytes(num_bytes: u64) -> String {
    let units = ["Bytes", "KB", "MB", "GB", "TB", "PB", "EB"];
    if num_bytes < 1024 {
        return format!("{} {}", num_bytes, units[0]);
    }
    let mut magnitude = (num_bytes as f64).ln() / 1024f64.ln();
    let unit_index = magnitude.floor() as usize;
    magnitude = 1024f64.powf(magnitude - magnitude.floor());
    if unit_index >= units.len() {
        format!("{:.2} {}", num_bytes, units.last().unwrap())
    } else {
        format!("{:.2} {}", magnitude, units[unit_index])
    }
}