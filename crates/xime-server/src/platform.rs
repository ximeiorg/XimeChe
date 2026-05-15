use std::path::PathBuf;
use std::sync::Mutex;

pub trait ClipboardProvider: Send + Sync {
    fn read(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>>;
    fn write(&self, content: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

pub trait ConfigDirProvider: Send + Sync {
    fn config_dir(&self) -> PathBuf;
}

pub struct InMemoryClipboard {
    content: Mutex<String>,
}

impl InMemoryClipboard {
    pub fn new() -> Self {
        Self {
            content: Mutex::new(String::new()),
        }
    }
}

impl Default for InMemoryClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardProvider for InMemoryClipboard {
    fn read(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.content.lock().unwrap().clone())
    }
    
    fn write(&self, content: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        *self.content.lock().unwrap() = content.to_string();
        Ok(())
    }
}

#[cfg(feature = "linux")]
pub use linux::*;

#[cfg(feature = "linux")]
mod linux {
    use super::*;
    use std::sync::Mutex;

    pub struct SystemClipboard {
        clipboard: Mutex<arboard::Clipboard>,
    }

    impl SystemClipboard {
        pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
            let clipboard = arboard::Clipboard::new()?;
            Ok(Self {
                clipboard: Mutex::new(clipboard),
            })
        }
    }

    impl ClipboardProvider for SystemClipboard {
        fn read(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            let mut clipboard = self.clipboard.lock().unwrap();
            clipboard.get_text()
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        }
        
        fn write(&self, content: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            let mut clipboard = self.clipboard.lock().unwrap();
            clipboard.set_text(content)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        }
    }

    pub fn get_config_dir() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
        PathBuf::from(home).join(".config/xime")
    }
}

#[cfg(feature = "windows")]
pub use windows::*;

#[cfg(feature = "windows")]
mod windows {
    use super::*;
    use std::sync::Mutex;

    pub struct SystemClipboard {
        clipboard: Mutex<arboard::Clipboard>,
    }

    impl SystemClipboard {
        pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
            let clipboard = arboard::Clipboard::new()?;
            Ok(Self {
                clipboard: Mutex::new(clipboard),
            })
        }
    }

    impl ClipboardProvider for SystemClipboard {
        fn read(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            let mut clipboard = self.clipboard.lock().unwrap();
            clipboard.get_text()
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        }
        
        fn write(&self, content: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            let mut clipboard = self.clipboard.lock().unwrap();
            clipboard.set_text(content)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        }
    }

    pub fn get_config_dir() -> PathBuf {
        let app_data = std::env::var("APPDATA")
            .or_else(|_| std::env::var("LOCALAPPDATA"))
            .unwrap_or_else(|_| {
                let user_profile = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:".to_string());
                format!("{}\\AppData\\Roaming", user_profile)
            });
        PathBuf::from(app_data).join("xime")
    }
}

pub struct DefaultConfigDir {
    base_dir: PathBuf,
}

impl DefaultConfigDir {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }
}

impl ConfigDirProvider for DefaultConfigDir {
    fn config_dir(&self) -> PathBuf {
        self.base_dir.clone()
    }
}

#[derive(Clone)]
pub struct PlatformProviders {
    pub clipboard: std::sync::Arc<dyn ClipboardProvider>,
    pub config_dir: std::sync::Arc<dyn ConfigDirProvider>,
}

impl std::fmt::Debug for PlatformProviders {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlatformProviders")
            .field("clipboard", &"Arc<dyn ClipboardProvider>")
            .field("config_dir", &"Arc<dyn ConfigDirProvider>")
            .finish()
    }
}

impl PlatformProviders {
    #[cfg(feature = "linux")]
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            clipboard: std::sync::Arc::new(SystemClipboard::new()?),
            config_dir: std::sync::Arc::new(DefaultConfigDir::new(get_config_dir())),
        })
    }
    
    #[cfg(feature = "windows")]
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            clipboard: std::sync::Arc::new(SystemClipboard::new()?),
            config_dir: std::sync::Arc::new(DefaultConfigDir::new(get_config_dir())),
        })
    }
}

impl Default for PlatformProviders {
    fn default() -> Self {
        Self::new().expect("Failed to initialize platform providers")
    }
}