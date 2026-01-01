// Build script for runtime-core
// - Auto-downloads FFmpeg libraries for ac-ffmpeg when video feature is enabled
// - Auto-downloads speaker diarization ONNX models when speaker-diarization feature is enabled
// - Sets FFMPEG_INCLUDE_DIR and FFMPEG_LIB_DIR environment variables

use std::env;
use std::path::PathBuf;
use std::fs;

fn main() {
    // Only setup FFmpeg if the video feature is enabled
    #[cfg(feature = "video")]
    setup_ffmpeg();

    // Download speaker diarization models if feature is enabled
    #[cfg(feature = "speaker-diarization")]
    setup_speaker_diarization_models();

    println!("cargo:rerun-if-changed=build.rs");
}

#[cfg(feature = "speaker-diarization")]
fn setup_speaker_diarization_models() {
    use std::process::Command;

    let out_dir = env::var("OUT_DIR").unwrap();
    let models_dir = PathBuf::from(&out_dir).join("speaker_diarization_models");

    fs::create_dir_all(&models_dir).expect("Failed to create speaker diarization models directory");

    let models = [
        (
            "segmentation-3.0.onnx",
            "https://github.com/thewh1teagle/pyannote-rs/releases/download/v0.1.0/segmentation-3.0.onnx"
        ),
        (
            "wespeaker_en_voxceleb_CAM++.onnx",
            "https://github.com/thewh1teagle/pyannote-rs/releases/download/v0.1.0/wespeaker_en_voxceleb_CAM++.onnx"
        ),
    ];

    for (filename, url) in models {
        let model_path = models_dir.join(filename);

        if model_path.exists() {
            println!("cargo:warning=Speaker diarization model {} already exists", filename);
            continue;
        }

        println!("cargo:warning=Downloading speaker diarization model: {} ...", filename);

        // Try curl first (available on most systems)
        let status = Command::new("curl")
            .args(&["-L", "-o", model_path.to_str().unwrap(), url])
            .status();

        if status.is_err() || !status.unwrap().success() {
            // Try wget as fallback
            let wget_status = Command::new("wget")
                .args(&["-O", model_path.to_str().unwrap(), url])
                .status();

            if wget_status.is_err() || !wget_status.unwrap().success() {
                // Try PowerShell on Windows
                #[cfg(target_os = "windows")]
                {
                    let ps_status = Command::new("powershell")
                        .args(&[
                            "-Command",
                            &format!(
                                "Invoke-WebRequest -Uri '{}' -OutFile '{}'",
                                url,
                                model_path.display()
                            ),
                        ])
                        .status();

                    if ps_status.is_err() || !ps_status.unwrap().success() {
                        panic!(
                            "Failed to download speaker diarization model {}. Please download manually from {} to {}",
                            filename, url, model_path.display()
                        );
                    }
                }

                #[cfg(not(target_os = "windows"))]
                panic!(
                    "Failed to download speaker diarization model {}. Please download manually from {} to {}",
                    filename, url, model_path.display()
                );
            }
        }

        println!("cargo:warning=Downloaded speaker diarization model: {}", filename);
    }

    // Expose model directory path to code via env var
    println!("cargo:rustc-env=SPEAKER_DIARIZATION_MODELS_DIR={}", models_dir.display());
    println!("cargo:warning=Speaker diarization models directory: {}", models_dir.display());
}


#[cfg(feature = "video")]
fn setup_ffmpeg() {
    // Check if static linking mode - need extra dependencies
    if env::var("FFMPEG_LIBS_MODE").map(|v| v == "static").unwrap_or(false) {
        // Static FFmpeg requires zlib, lzma, and other compression libraries
        println!("cargo:rustc-link-lib=z");
        println!("cargo:rustc-link-lib=lzma");
        println!("cargo:rustc-link-lib=bz2");
        // X11/VDPAU for hardware acceleration (Linux only)
        #[cfg(target_os = "linux")]
        {
            println!("cargo:rustc-link-lib=X11");
            println!("cargo:rustc-link-lib=vdpau");
        }
        // OpenSSL for RTMPS/HTTPS network protocols
        println!("cargo:rustc-link-lib=ssl");
        println!("cargo:rustc-link-lib=crypto");
        // Additional network protocol dependencies
        #[cfg(target_os = "linux")]
        {
            // GnuTLS alternative (if FFmpeg was built with GnuTLS instead of OpenSSL)
            // Uncomment if needed:
            // println!("cargo:rustc-link-lib=gnutls");
            
            // librtmp for native RTMP support (if FFmpeg was built with librtmp)
            // Uncomment if needed:
            // println!("cargo:rustc-link-lib=rtmp");
        }
    }

    // Check if user already has FFMPEG_INCLUDE_DIR set
    if env::var("FFMPEG_INCLUDE_DIR").is_ok() {
        println!("cargo:warning=Using existing FFMPEG_INCLUDE_DIR from environment");
        return;
    }

    let target = env::var("TARGET").unwrap();
    let out_dir = env::var("OUT_DIR").unwrap();
    let ffmpeg_dir = PathBuf::from(&out_dir).join("ffmpeg");

    // Create ffmpeg directory if it doesn't exist
    fs::create_dir_all(&ffmpeg_dir).expect("Failed to create ffmpeg directory");

    // Download and extract FFmpeg based on target platform
    let (include_dir, lib_dir) = match target.as_str() {
        t if t.contains("linux") => download_ffmpeg_linux(&ffmpeg_dir),
        t if t.contains("darwin") || t.contains("macos") => download_ffmpeg_macos(&ffmpeg_dir),
        t if t.contains("windows") => download_ffmpeg_windows(&ffmpeg_dir),
        _ => {
            println!("cargo:warning=Unsupported platform for auto-download: {}. Please set FFMPEG_INCLUDE_DIR and FFMPEG_LIB_DIR manually.", target);
            return;
        }
    };

    // Set environment variables for ac-ffmpeg
    println!("cargo:rustc-env=FFMPEG_INCLUDE_DIR={}", include_dir.display());
    println!("cargo:rustc-env=FFMPEG_LIB_DIR={}", lib_dir.display());

    // Also set them for the current build
    env::set_var("FFMPEG_INCLUDE_DIR", &include_dir);
    env::set_var("FFMPEG_LIB_DIR", &lib_dir);

    println!("cargo:warning=FFmpeg auto-configured: include={}, lib={}", include_dir.display(), lib_dir.display());
}

#[cfg(feature = "video")]
fn download_ffmpeg_linux(ffmpeg_dir: &PathBuf) -> (PathBuf, PathBuf) {
    use std::process::Command;

    let include_dir = ffmpeg_dir.join("include");
    let lib_dir = ffmpeg_dir.join("lib");

    // Check if already downloaded
    if include_dir.exists() && lib_dir.exists() {
        println!("cargo:warning=Using cached FFmpeg from {}", ffmpeg_dir.display());
        return (include_dir, lib_dir);
    }

    println!("cargo:warning=Auto-downloading FFmpeg for Linux...");

    // Try to use system package manager to install development files
    // This is a build-time dependency, so we can use system packages
    let status = Command::new("sh")
        .arg("-c")
        .arg("command -v pkg-config")
        .status();

    if status.is_ok() && status.unwrap().success() {
        // Check if FFmpeg is already installed via pkg-config
        let pc_status = Command::new("pkg-config")
            .args(&["--exists", "libavcodec", "libavformat", "libavutil"])
            .status();

        if pc_status.is_ok() && pc_status.unwrap().success() {
            // Get paths from pkg-config
            let include_output = Command::new("pkg-config")
                .args(&["--variable=includedir", "libavcodec"])
                .output()
                .expect("Failed to run pkg-config");

            let lib_output = Command::new("pkg-config")
                .args(&["--variable=libdir", "libavcodec"])
                .output()
                .expect("Failed to run pkg-config");

            let pkg_include = String::from_utf8_lossy(&include_output.stdout).trim().to_string();
            let pkg_lib = String::from_utf8_lossy(&lib_output.stdout).trim().to_string();

            if !pkg_include.is_empty() && !pkg_lib.is_empty() {
                println!("cargo:warning=Found system FFmpeg via pkg-config");
                return (PathBuf::from(pkg_include), PathBuf::from(pkg_lib));
            }
        }
    }

    println!("cargo:warning=System FFmpeg not found. Please install FFmpeg development packages:");
    println!("cargo:warning=  Ubuntu/Debian: sudo apt-get install libavcodec-dev libavformat-dev libavutil-dev libswscale-dev");
    println!("cargo:warning=  Fedora/RHEL: sudo dnf install ffmpeg-devel");
    println!("cargo:warning=  Arch: sudo pacman -S ffmpeg");
    panic!("FFmpeg development libraries not found. Please install them or set FFMPEG_INCLUDE_DIR and FFMPEG_LIB_DIR manually.");
}

#[cfg(feature = "video")]
fn download_ffmpeg_macos(ffmpeg_dir: &PathBuf) -> (PathBuf, PathBuf) {
    use std::process::Command;

    let include_dir = ffmpeg_dir.join("include");
    let lib_dir = ffmpeg_dir.join("lib");

    // Check if already downloaded
    if include_dir.exists() && lib_dir.exists() {
        println!("cargo:warning=Using cached FFmpeg from {}", ffmpeg_dir.display());
        return (include_dir, lib_dir);
    }

    println!("cargo:warning=Auto-configuring FFmpeg for macOS...");

    // Try to find FFmpeg via Homebrew
    let brew_prefix_output = Command::new("brew")
        .args(&["--prefix", "ffmpeg"])
        .output();

    if let Ok(output) = brew_prefix_output {
        if output.status.success() {
            let brew_prefix = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let brew_include = PathBuf::from(&brew_prefix).join("include");
            let brew_lib = PathBuf::from(&brew_prefix).join("lib");

            if brew_include.exists() && brew_lib.exists() {
                println!("cargo:warning=Found FFmpeg via Homebrew at {}", brew_prefix);
                return (brew_include, brew_lib);
            }
        }
    }

    println!("cargo:warning=FFmpeg not found via Homebrew. Please install:");
    println!("cargo:warning=  brew install ffmpeg");
    panic!("FFmpeg not found. Please install via Homebrew or set FFMPEG_INCLUDE_DIR and FFMPEG_LIB_DIR manually.");
}

#[cfg(feature = "video")]
fn download_ffmpeg_windows(ffmpeg_dir: &PathBuf) -> (PathBuf, PathBuf) {
    let include_dir = ffmpeg_dir.join("include");
    let lib_dir = ffmpeg_dir.join("lib");

    // Check if already downloaded
    if include_dir.exists() && lib_dir.exists() {
        println!("cargo:warning=Using cached FFmpeg from {}", ffmpeg_dir.display());
        return (include_dir, lib_dir);
    }

    println!("cargo:warning=Auto-downloading FFmpeg for Windows...");

    // Download pre-built FFmpeg from gyan.dev (popular source for Windows FFmpeg builds)
    // Using shared builds (smaller and easier to work with)
    let download_url = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip".to_string();

    let zip_path = ffmpeg_dir.join("ffmpeg.zip");
    let extract_dir = ffmpeg_dir.join("extracted");

    // Download FFmpeg zip
    println!("cargo:warning=Downloading FFmpeg from {}", download_url);
    let status = std::process::Command::new("powershell")
        .args(&[
            "-Command",
            &format!(
                "Invoke-WebRequest -Uri '{}' -OutFile '{}'",
                download_url,
                zip_path.display()
            ),
        ])
        .status();

    if status.is_err() || !status.unwrap().success() {
        // Try with curl as fallback
        let curl_status = std::process::Command::new("curl")
            .args(&["-L", "-o", zip_path.to_str().unwrap(), &download_url])
            .status();

        if curl_status.is_err() || !curl_status.unwrap().success() {
            panic!("Failed to download FFmpeg. Please download manually from {} and extract to {}", download_url, ffmpeg_dir.display());
        }
    }

    // Extract zip
    println!("cargo:warning=Extracting FFmpeg...");
    let extract_status = std::process::Command::new("powershell")
        .args(&[
            "-Command",
            &format!(
                "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
                zip_path.display(),
                extract_dir.display()
            ),
        ])
        .status();

    if extract_status.is_err() || !extract_status.unwrap().success() {
        panic!("Failed to extract FFmpeg zip. Please extract manually.");
    }

    // Find the extracted directory (usually ffmpeg-VERSION-essentials_build)
    let entries = fs::read_dir(&extract_dir).expect("Failed to read extract directory");
    let mut ffmpeg_build_dir = None;
    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_dir() && path.file_name().unwrap().to_str().unwrap().starts_with("ffmpeg-") {
                ffmpeg_build_dir = Some(path);
                break;
            }
        }
    }

    let ffmpeg_build_dir = ffmpeg_build_dir.expect("Failed to find extracted FFmpeg directory");

    // Copy include and lib directories
    let src_include = ffmpeg_build_dir.join("include");
    let src_lib = ffmpeg_build_dir.join("lib");

    copy_dir_recursive(&src_include, &include_dir).expect("Failed to copy include directory");
    copy_dir_recursive(&src_lib, &lib_dir).expect("Failed to copy lib directory");

    println!("cargo:warning=FFmpeg extracted to {}", ffmpeg_dir.display());

    // Clean up zip file
    let _ = fs::remove_file(&zip_path);

    (include_dir, lib_dir)
}

#[cfg(feature = "video")]
fn copy_dir_recursive(src: &PathBuf, dst: &PathBuf) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
