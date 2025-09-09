mod cli;
mod vcxproj;

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
use walkdir::WalkDir;

use cli::{Cli, Commands};
use vcxproj::{FilterFile, VcxprojFile, ProjectStructure};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Add { extension, project, directory, recursive } => {
            add_files_to_project(extension, project, directory, recursive)?;
        }
        Commands::Delete { project, target, extension, yes } => {
            delete_from_project(project, target, extension, yes)?;
        }
        Commands::View { project, files_only } => {
            view_project_structure(project, files_only)?;
        }
        Commands::Rename { project, from, to, yes } => {
            rename_filter_in_project(project, from, to, yes)?;
        }
        Commands::AddInclude { project, path } => {
            add_include_directory(project, path)?;
        }
        Commands::AddLibDir { project, path } => {
            add_library_directory(project, path)?;
        }
        Commands::AddLib { project, name } => {
            add_library_dependency(project, name)?;
        }
    }

    Ok(())
}

fn add_files_to_project(
    extension: String,
    project_path: PathBuf,
    directory: Option<PathBuf>,
    recursive: bool,
) -> Result<()> {
    // Determine the directory to scan
    let scan_dir = directory.unwrap_or_else(|| {
        project_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf()
    });

    println!("Scanning directory: {}", scan_dir.display());
    println!("Looking for *.{} files", extension);

    // Find all files with the specified extension
    let mut files_to_add = Vec::new();
    
    let walker = if recursive {
        WalkDir::new(&scan_dir)
    } else {
        WalkDir::new(&scan_dir).max_depth(1)
    };

    for entry in walker {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();
        
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext.to_string_lossy().eq_ignore_ascii_case(&extension) {
                    // Make path relative to project directory if possible
                    let relative_path = if let Some(project_dir) = project_path.parent() {
                        match path.strip_prefix(project_dir) {
                            Ok(rel) => rel.to_path_buf(),
                            Err(_) => path.to_path_buf(),
                        }
                    } else {
                        path.to_path_buf()
                    };
                    files_to_add.push(relative_path);
                }
            }
        }
    }

    if files_to_add.is_empty() {
        println!("No *.{} files found in {}", extension, scan_dir.display());
        return Ok(());
    }

    println!("Found {} files to add:", files_to_add.len());
    for file in &files_to_add {
        println!("  - {}", file.display());
    }

    // Load and update the .vcxproj file
    println!("\nUpdating project file: {}", project_path.display());
    let mut vcxproj = VcxprojFile::load(&project_path)?;
    vcxproj.add_source_files(&files_to_add)?;
    vcxproj.save()?;
    println!("Successfully updated {}", project_path.display());

    // Update the .vcxproj.filters file if it exists
    let filter_path = project_path.with_extension("vcxproj.filters");
    if filter_path.exists() {
        println!("Updating filter file: {}", filter_path.display());
        let mut filter_file = FilterFile::load(&filter_path)?;
        filter_file.add_source_files(&files_to_add)?;
        filter_file.save()?;
        println!("Successfully updated {}", filter_path.display());
    } else {
        println!("Filter file not found: {}", filter_path.display());
        println!("Creating basic filter file...");
        
        // Create a basic filter file
        let filter_content = create_basic_filter_file(&files_to_add)?;
        std::fs::write(&filter_path, filter_content)
            .context("Failed to create filter file")?;
        println!("Created {}", filter_path.display());
    }

    println!("\n✅ Project files updated successfully!");
    Ok(())
}

fn create_basic_filter_file(files: &[PathBuf]) -> Result<String> {
    use std::collections::HashMap;
    
    let mut filters = HashMap::new();
    
    // Collect unique directories
    for file in files {
        if let Some(parent) = file.parent() {
            let filter_name = parent.to_string_lossy().replace('/', "\\");
            if !filter_name.is_empty() {
                filters.insert(filter_name, uuid::Uuid::new_v4());
            }
        }
    }

    let mut content = String::new();
    content.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    content.push_str("<Project ToolsVersion=\"4.0\" xmlns=\"http://schemas.microsoft.com/developer/msbuild/2003\">\n");
    
    // Add default filters
    content.push_str("  <ItemGroup>\n");
    content.push_str("    <Filter Include=\"Source Files\">\n");
    content.push_str("      <UniqueIdentifier>{4FC737F1-C7A5-4376-A066-2A32D752A2FF}</UniqueIdentifier>\n");
    content.push_str("      <Extensions>cpp;c;cc;cxx;c++;cppm;ixx;def;odl;idl;hpj;bat;asm;asmx</Extensions>\n");
    content.push_str("    </Filter>\n");
    content.push_str("    <Filter Include=\"Header Files\">\n");
    content.push_str("      <UniqueIdentifier>{93995380-89BD-4b04-88EB-625FBE52EBFB}</UniqueIdentifier>\n");
    content.push_str("      <Extensions>h;hh;hpp;hxx;h++;hm;inl;inc;ipp;xsd</Extensions>\n");
    content.push_str("    </Filter>\n");
    
    // Add custom filters for directories
    for (filter_name, uuid) in &filters {
        content.push_str(&format!("    <Filter Include=\"{}\">\n", filter_name));
        content.push_str(&format!("      <UniqueIdentifier>{{{}}}</UniqueIdentifier>\n", uuid.to_string().to_uppercase()));
        content.push_str("    </Filter>\n");
    }
    
    content.push_str("  </ItemGroup>\n");
    
    // Add file entries
    content.push_str("  <ItemGroup>\n");
    for file in files {
        if let Some(ext) = file.extension() {
            if ext == "c" || ext == "cpp" || ext == "cc" || ext == "cxx" {
                let include_path = file.to_string_lossy().replace('/', "\\");
                content.push_str(&format!("    <ClCompile Include=\"{}\">\n", include_path));
                
                if let Some(parent) = file.parent() {
                    let filter_name = parent.to_string_lossy().replace('/', "\\");
                    if !filter_name.is_empty() {
                        content.push_str(&format!("      <Filter>{}</Filter>\n", filter_name));
                    } else {
                        content.push_str("      <Filter>Source Files</Filter>\n");
                    }
                } else {
                    content.push_str("      <Filter>Source Files</Filter>\n");
                }
                
                content.push_str("    </ClCompile>\n");
            }
        }
    }
    content.push_str("  </ItemGroup>\n");
    content.push_str("</Project>\n");

    Ok(content)
}

fn delete_from_project(
    project_path: PathBuf,
    target: Option<String>,
    extension: Option<String>,
    yes: bool,
) -> Result<()> {
    println!("Analyzing project: {}", project_path.display());
    
    // Validate arguments
    if target.is_none() && extension.is_none() {
        return Err(anyhow::anyhow!("Either --target or --extension must be specified"));
    }
    
    let target_str = target.as_deref().unwrap_or("");
    let target_display = if let Some(ref ext) = extension {
        format!("all *.{} files", ext)
    } else {
        target_str.to_string()
    };
    
    // Load the project file
    let mut vcxproj = VcxprojFile::load(&project_path)?;
    
    // Preview what will be deleted
    let original_content = vcxproj.content.clone();
    let deleted_files = vcxproj.delete_files(target_str, extension.as_deref())?;
    vcxproj.content = original_content; // Restore for confirmation
    
    if deleted_files.is_empty() {
        println!("No files found matching: {}", target_display);
        return Ok(());
    }
    
    // Show what will be deleted
    println!("\n📁 Files to be removed from project:");
    for file in &deleted_files {
        println!("  - {}", file);
    }
    
    // Check filter file as well
    let filter_path = project_path.with_extension("vcxproj.filters");
    let mut preview_filters = Vec::new();
    if filter_path.exists() {
        let mut filter_file = FilterFile::load(&filter_path)?;
        let original_filter_content = filter_file.content.clone();
        let (_, deleted_filters) = filter_file.delete_files_and_filters(target_str, extension.as_deref())?;
        preview_filters = deleted_filters;
        filter_file.content = original_filter_content; // Restore for confirmation
    }
    
    if !preview_filters.is_empty() {
        println!("\n📂 Filters to be removed:");
        for filter in &preview_filters {
            println!("  - {}", filter);
        }
    }
    
    // Confirm deletion
    if !yes {
        print!("\nRemove {} items from project? [y/N]: ", deleted_files.len());
        use std::io::{self, Write};
        io::stdout().flush()?;
        
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();
        
        if input != "y" && input != "yes" {
            println!("Operation cancelled.");
            return Ok(());
        }
    }
    
    // Perform the deletion
    println!("\nUpdating project file: {}", project_path.display());
    vcxproj.delete_files(target_str, extension.as_deref())?;
    vcxproj.save()?;
    println!("Successfully updated {}", project_path.display());
    
    // Update filter file if it exists
    if filter_path.exists() {
        println!("Updating filter file: {}", filter_path.display());
        let mut filter_file = FilterFile::load(&filter_path)?;
        filter_file.delete_files_and_filters(target_str, extension.as_deref())?;
        filter_file.save()?;
        println!("Successfully updated {}", filter_path.display());
    }
    
    println!("\n🗑️  Successfully removed {} files from project!", deleted_files.len());
    Ok(())
}

fn view_project_structure(
    project_path: PathBuf,
    files_only: bool,
) -> Result<()> {
    // Load and parse the project structure
    let structure = ProjectStructure::from_project(&project_path)?;
    
    // Display the tree structure (extensions always shown)
    let tree_output = structure.display_tree(files_only, true);
    print!("{}", tree_output);
    
    // Show summary
    let file_count = structure.files.len();
    let filter_count = structure.filters.len();
    
    if file_count == 0 && filter_count == 0 {
        println!("📊 Project summary: Empty project");
    } else {
        println!("📊 Project summary: {} files", file_count);
        if !files_only && filter_count > 0 {
            println!("   {} filters", filter_count);
        }
    }
    
    Ok(())
}

fn rename_filter_in_project(
    project_path: PathBuf,
    from: String,
    to: String,
    yes: bool,
) -> Result<()> {
    println!("Analyzing project: {}", project_path.display());
    
    // Check if filter file exists
    let filter_path = project_path.with_extension("vcxproj.filters");
    if !filter_path.exists() {
        return Err(anyhow::anyhow!("Filter file not found: {}", filter_path.display()));
    }
    
    // Load filter file
    let mut filter_file = FilterFile::load(&filter_path)?;
    
    // Attempt to rename the filter
    let (target_exists, renamed_files) = filter_file.rename_filter(&from, &to)?;
    
    if renamed_files.is_empty() {
        println!("No files found in filter '{}'", from);
        return Ok(());
    }
    
    if target_exists {
        // Conflict detected - ask for merge confirmation
        println!("⚠️  Conflict detected!");
        println!("Filter '{}' already exists in the project.", to);
        println!("Files in '{}' filter:", from);
        for file in &renamed_files {
            println!("  - {}", file);
        }
        
        if !yes {
            print!("\nMerge '{}' into existing '{}' filter? [y/N]: ", from, to);
            use std::io::{self, Write};
            io::stdout().flush()?;
            
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let input = input.trim().to_lowercase();
            
            if input != "y" && input != "yes" {
                println!("Operation cancelled.");
                return Ok(());
            }
        }
        
        // Reload filter file (since rename_filter modified it) and perform merge
        let mut filter_file = FilterFile::load(&filter_path)?;
        let moved_files = filter_file.merge_filters(&from, &to)?;
        filter_file.save()?;
        
        println!("✅ Successfully merged filter '{}' into '{}'", from, to);
        println!("📁 {} files moved:", moved_files.len());
        for file in &moved_files {
            println!("  - {} → {}", file, to);
        }
    } else {
        // Simple rename - no conflict
        filter_file.save()?;
        
        println!("✅ Successfully renamed filter '{}' to '{}'", from, to);
        println!("📁 {} files moved:", renamed_files.len());
        for file in &renamed_files {
            println!("  - {} → {}", file, to);
        }
    }
    
    println!("Successfully updated {}", filter_path.display());
    Ok(())
}

fn add_include_directory(project_path: PathBuf, include_path: String) -> Result<()> {
    println!("Adding include directory '{}' to project: {}", include_path, project_path.display());
    
    let mut vcxproj = VcxprojFile::load(&project_path)?;
    let modified_configs = vcxproj.add_include_directory(&include_path)?;
    vcxproj.save()?;
    
    if modified_configs.is_empty() {
        println!("⚠️  No configurations found to modify");
    } else {
        println!("✅ Successfully added include directory to {} configurations:", modified_configs.len());
        for config in &modified_configs {
            println!("  - {}", config);
        }
    }
    
    Ok(())
}

fn add_library_directory(project_path: PathBuf, lib_path: String) -> Result<()> {
    println!("Adding library directory '{}' to project: {}", lib_path, project_path.display());
    
    let mut vcxproj = VcxprojFile::load(&project_path)?;
    let modified_configs = vcxproj.add_library_directory(&lib_path)?;
    vcxproj.save()?;
    
    if modified_configs.is_empty() {
        println!("⚠️  No configurations found to modify");
    } else {
        println!("✅ Successfully added library directory to {} configurations:", modified_configs.len());
        for config in &modified_configs {
            println!("  - {}", config);
        }
    }
    
    Ok(())
}

fn add_library_dependency(project_path: PathBuf, lib_name: String) -> Result<()> {
    println!("Adding library dependency '{}' to project: {}", lib_name, project_path.display());
    
    let mut vcxproj = VcxprojFile::load(&project_path)?;
    let modified_configs = vcxproj.add_library_dependency(&lib_name)?;
    vcxproj.save()?;
    
    if modified_configs.is_empty() {
        println!("⚠️  No configurations found to modify");
    } else {
        println!("✅ Successfully added library dependency to {} configurations:", modified_configs.len());
        for config in &modified_configs {
            println!("  - {}", config);
        }
    }
    
    Ok(())
}
