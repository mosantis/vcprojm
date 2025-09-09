mod cli;
mod vcxproj;

use anyhow::{Context, Result};
use clap::Parser;
use regex::Regex;
use std::path::PathBuf;
use walkdir::WalkDir;

use cli::{Cli, Commands};
use vcxproj::{FilterFile, VcxprojFile, ProjectStructure};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Add { extension, project, directory, recursive, regex, not, dryrun } => {
            add_files_to_project(extension, project, directory, recursive, regex, not, dryrun)?;
        }
        Commands::Delete { project, target, extension, yes, regex, not, dryrun } => {
            delete_from_project(project, target, extension, yes, regex, not, dryrun)?;
        }
        Commands::View { project, files_only, level } => {
            view_project_structure(project, files_only, level)?;
        }
        Commands::Rename { project, from, to, yes, dryrun } => {
            rename_filter_in_project(project, from, to, yes, dryrun)?;
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
    regex_pattern: Option<String>,
    negate: bool,
    dryrun: bool,
) -> Result<()> {
    // Determine the directory to scan
    let scan_dir = directory.unwrap_or_else(|| {
        project_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf()
    });

    println!("Scanning directory: {}", scan_dir.display());
    
    match (&regex_pattern, negate) {
        (Some(ref pattern), true) => println!("Looking for *.{} files in paths NOT matching regex: {}", extension, pattern),
        (Some(ref pattern), false) => println!("Looking for *.{} files in paths matching regex: {}", extension, pattern),
        (None, true) => println!("Looking for *.{} files (negation has no effect without regex)", extension),
        (None, false) => println!("Looking for *.{} files", extension),
    }

    // Compile regex pattern if provided
    let compiled_regex = if let Some(ref pattern) = regex_pattern {
        Some(Regex::new(pattern).context("Invalid regex pattern")?)
    } else {
        None
    };

    // Find all files with the specified extension, filtered by path regex if provided
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
            // First check if file has the correct extension
            let has_extension = if let Some(ext) = path.extension() {
                ext.to_string_lossy().eq_ignore_ascii_case(&extension)
            } else {
                false
            };
            
            if !has_extension {
                continue;
            }
            
            // Then check if path matches regex (if provided) with negation support
            let path_matches = if let Some(ref regex) = compiled_regex {
                // Get the relative path from scan_dir to apply regex against
                let relative_to_scan = path.strip_prefix(&scan_dir).unwrap_or(path);
                let path_str = relative_to_scan.to_string_lossy();
                let regex_matches = regex.is_match(&path_str);
                
                if negate {
                    !regex_matches // Include files that DON'T match the regex
                } else {
                    regex_matches // Include files that DO match the regex
                }
            } else {
                true // No regex means all paths match (negation has no effect)
            };
            
            if path_matches {
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

    if files_to_add.is_empty() {
        if let Some(ref pattern) = regex_pattern {
            println!("No *.{} files found in paths matching regex '{}' in {}", extension, pattern, scan_dir.display());
        } else {
            println!("No *.{} files found in {}", extension, scan_dir.display());
        }
        return Ok(());
    }

    println!("Found {} files to add:", files_to_add.len());
    for file in &files_to_add {
        println!("  - {}", file.display());
    }

    if dryrun {
        println!("\n🔍 DRY RUN - No files were modified");
        println!("Would update project file: {}", project_path.display());
        
        let filter_path = project_path.with_extension("vcxproj.filters");
        if filter_path.exists() {
            println!("Would update filter file: {}", filter_path.display());
        } else {
            println!("Would create filter file: {}", filter_path.display());
        }
        
        println!("✨ Dry run completed - {} files would be added", files_to_add.len());
        return Ok(());
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
    regex_pattern: Option<String>,
    negate: bool,
    dryrun: bool,
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
    
    // Compile regex pattern if provided
    let compiled_regex = if let Some(ref pattern) = regex_pattern {
        Some(Regex::new(pattern).context("Invalid regex pattern")?)
    } else {
        None
    };

    // Preview what will be deleted
    let original_content = vcxproj.content.clone();
    let all_deleted_files = vcxproj.delete_files(target_str, extension.as_deref())?;
    vcxproj.content = original_content; // Restore for confirmation
    
    // Apply regex filtering if provided with negation support
    let deleted_files: Vec<String> = if let Some(ref regex) = compiled_regex {
        all_deleted_files.into_iter()
            .filter(|file_path| {
                let regex_matches = regex.is_match(file_path);
                if negate {
                    !regex_matches // Delete files that DON'T match the regex
                } else {
                    regex_matches // Delete files that DO match the regex
                }
            })
            .collect()
    } else {
        all_deleted_files
    };
    
    if deleted_files.is_empty() {
        match (&regex_pattern, negate) {
            (Some(ref pattern), true) => println!("No files found matching: {} with regex filter NOT matching: {}", target_display, pattern),
            (Some(ref pattern), false) => println!("No files found matching: {} with regex filter: {}", target_display, pattern),
            (None, _) => println!("No files found matching: {}", target_display),
        }
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
        let (_, all_deleted_filters) = filter_file.delete_files_and_filters(target_str, extension.as_deref())?;
        // Apply the same regex filtering to filters (optional, may not be needed)
        preview_filters = all_deleted_filters;
        filter_file.content = original_filter_content; // Restore for confirmation
    }
    
    if !preview_filters.is_empty() {
        println!("\n📂 Filters to be removed:");
        for filter in &preview_filters {
            println!("  - {}", filter);
        }
    }
    
    if dryrun {
        println!("\n🔍 DRY RUN - No files were modified");
        println!("Would remove {} files from project file: {}", deleted_files.len(), project_path.display());
        
        if filter_path.exists() {
            if !preview_filters.is_empty() {
                println!("Would remove {} filters from filter file: {}", preview_filters.len(), filter_path.display());
            }
            println!("Would update filter file: {}", filter_path.display());
        }
        
        println!("✨ Dry run completed - {} files would be removed", deleted_files.len());
        return Ok(());
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
    level: Option<usize>,
) -> Result<()> {
    // Load and parse the project structure
    let structure = ProjectStructure::from_project(&project_path)?;
    
    // Display the tree structure (extensions always shown)
    let tree_output = structure.display_tree(files_only, true, level);
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
    dryrun: bool,
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
    
    if dryrun {
        println!("\n🔍 DRY RUN - No files were modified");
        if target_exists {
            println!("Would merge filter '{}' into existing filter '{}'", from, to);
            println!("Files that would be moved from '{}' filter:", from);
            for file in &renamed_files {
                println!("  - {} → {}", file, to);
            }
        } else {
            println!("Would rename filter '{}' to '{}'", from, to);
            println!("Files that would be moved:");
            for file in &renamed_files {
                println!("  - {} → {}", file, to);
            }
        }
        println!("Would update filter file: {}", filter_path.display());
        println!("✨ Dry run completed - {} files would be moved", renamed_files.len());
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
