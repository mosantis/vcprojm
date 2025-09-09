use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "vsprojm")]
#[command(about = "A tool for manipulating Visual Studio project files")]
#[command(version = "0.1.0")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Add files of specified extension to the project
    #[command(name = "add", visible_alias = "a")]
    Add {
        /// File extension to add (e.g., "c", "cpp") or regex pattern when used with --regex
        #[arg(short, long)]
        extension: String,
        
        /// Path to the .vcxproj file
        #[arg(short, long)]
        project: PathBuf,
        
        /// Root directory to scan for files (defaults to project directory)
        #[arg(short, long)]
        directory: Option<PathBuf>,
        
        /// Include subdirectories in scan
        #[arg(short, long, default_value_t = true)]
        recursive: bool,
        
        /// Treat extension as a regex pattern instead of a file extension
        #[arg(short = 'x', long)]
        regex: bool,
    },
    
    /// Delete files or folders from the project
    #[command(name = "delete", visible_alias = "del")]
    Delete {
        /// Path to the .vcxproj file
        #[arg(short, long)]
        project: PathBuf,
        
        /// Filter name or file path to delete (e.g., "Header Files", "src/utils", "main.c")
        #[arg(short, long)]
        target: Option<String>,
        
        /// Delete by file extension instead of specific path
        #[arg(short, long)]
        extension: Option<String>,
        
        /// Confirm deletion without prompting
        #[arg(short = 'y', long)]
        yes: bool,
    },
    
    /// View project structure as it appears in Visual Studio
    #[command(name = "view", visible_alias = "v")]
    View {
        /// Path to the .vcxproj file
        #[arg(short, long)]
        project: PathBuf,
        
        /// Show only files (don't show empty filters)
        #[arg(short, long)]
        files_only: bool,
        
        /// Maximum hierarchy levels to display (0=folders only, default=all levels)
        #[arg(short, long)]
        level: Option<usize>,
    },
    
    /// Rename folders/filters in the project
    #[command(name = "rename", visible_alias = "ren")]
    Rename {
        /// Path to the .vcxproj file
        #[arg(short, long)]
        project: PathBuf,
        
        /// Current folder/filter name to rename
        #[arg(short, long)]
        from: String,
        
        /// New folder/filter name
        #[arg(short, long)]
        to: String,
        
        /// Skip confirmation prompts
        #[arg(short = 'y', long)]
        yes: bool,
    },
    
    /// Add include directory to all configurations
    #[command(name = "add-incdir", visible_alias = "incdir")]
    AddInclude {
        /// Path to the .vcxproj file
        #[arg(short, long)]
        project: PathBuf,
        
        /// Include directory path
        #[arg(short, long)]
        path: String,
    },
    
    /// Add library directory to all configurations
    #[command(name = "add-libdir", visible_alias = "libdir")]
    AddLibDir {
        /// Path to the .vcxproj file
        #[arg(short, long)]
        project: PathBuf,
        
        /// Library directory path
        #[arg(short, long)]
        path: String,
    },
    
    /// Add library file to all configurations
    #[command(name = "add-lib", visible_alias = "lib")]
    AddLib {
        /// Path to the .vcxproj file
        #[arg(short, long)]
        project: PathBuf,
        
        /// Library file name (e.g., "opengl32.lib")
        #[arg(short, long)]
        name: String,
    },
}