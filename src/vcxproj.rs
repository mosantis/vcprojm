use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct VcxprojFile {
    pub path: PathBuf,
    pub content: String,
}

#[derive(Debug)]
pub struct FilterFile {
    pub path: PathBuf,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ProjectFile {
    pub path: String,
    pub filter: Option<String>,
}

#[derive(Debug)]
pub struct ProjectStructure {
    pub name: String,
    pub files: Vec<ProjectFile>,
    pub filters: HashMap<String, Vec<String>>, // filter name -> files in filter
}

impl VcxprojFile {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read vcxproj file: {}", path.display()))?;
        
        Ok(Self { path, content })
    }

    pub fn add_source_files(&mut self, files: &[PathBuf]) -> Result<()> {
        // Simple string-based approach to add files
        let mut new_entries = String::new();
        
        for file in files {
            if let Some(ext) = file.extension() {
                if ext == "c" || ext == "cpp" || ext == "cc" || ext == "cxx" {
                    let include_path = file.to_string_lossy().replace('/', "\\");
                    new_entries.push_str(&format!("    <ClCompile Include=\"{}\" />\n", include_path));
                }
            }
        }

        if new_entries.is_empty() {
            return Ok(());
        }

        // Find the ClCompile ItemGroup or create one
        if let Some(pos) = self.content.find("<ClCompile Include=") {
            // Find the end of this ItemGroup
            let before_pos = &self.content[..pos];
            if let Some(itemgroup_start) = before_pos.rfind("<ItemGroup>") {
                let after_itemgroup = &self.content[itemgroup_start..];
                if let Some(itemgroup_end) = after_itemgroup.find("</ItemGroup>") {
                    let insertion_point = itemgroup_start + itemgroup_end;
                    self.content.insert_str(insertion_point, &new_entries);
                    return Ok(());
                }
            }
        }

        // If no ClCompile ItemGroup found, create one before the closing Project tag
        if let Some(pos) = self.content.rfind("</Project>") {
            let itemgroup = format!(
                "  <ItemGroup>\n{}\n  </ItemGroup>\n",
                new_entries.trim_end()
            );
            self.content.insert_str(pos, &itemgroup);
        }

        Ok(())
    }

    pub fn delete_files(&mut self, target: &str, extension: Option<&str>) -> Result<Vec<String>> {
        let mut deleted_files = Vec::new();
        let mut lines: Vec<String> = self.content.lines().map(|s| s.to_string()).collect();
        let mut i = 0;
        
        while i < lines.len() {
            let line = &lines[i];
            
            // Look for ClCompile entries
            if line.trim_start().starts_with("<ClCompile Include=\"") {
                let should_delete = if let Some(ext) = extension {
                    // Delete by extension
                    line.contains(&format!(".{}", ext))
                } else {
                    // Delete by specific file path or folder
                    if target.ends_with('/') || target.ends_with('\\') {
                        // Folder deletion - check if file is in this folder
                        let target_normalized = target.replace('/', "\\");
                        line.contains(&target_normalized) || line.contains(&target.replace('\\', "/"))
                    } else {
                        // Specific file deletion
                        line.contains(target)
                    }
                };
                
                if should_delete {
                    // Extract filename for reporting
                    if let Some(start) = line.find("Include=\"") {
                        if let Some(end) = line[start + 9..].find('"') {
                            let filename = &line[start + 9..start + 9 + end];
                            deleted_files.push(filename.to_string());
                        }
                    }
                    
                    // Remove the ClCompile line
                    if line.trim().ends_with("/>") {
                        // Self-closing tag
                        lines.remove(i);
                    } else {
                        // Multi-line entry, find the closing tag
                        lines.remove(i);
                        while i < lines.len() && !lines[i].trim().ends_with("</ClCompile>") {
                            lines.remove(i);
                        }
                        if i < lines.len() {
                            lines.remove(i); // Remove closing tag
                        }
                    }
                } else {
                    i += 1;
                }
            } else {
                i += 1;
            }
        }
        
        self.content = lines.join("\n");
        Ok(deleted_files)
    }

    pub fn get_project_files(&self) -> Result<Vec<ProjectFile>> {
        let mut files = Vec::new();
        let lines: Vec<&str> = self.content.lines().collect();
        
        for line in &lines {
            let trimmed = line.trim_start();
            if trimmed.starts_with("<ClCompile Include=\"") {
                if let Some(start) = line.find("Include=\"") {
                    if let Some(end) = line[start + 9..].find('"') {
                        let file_path = &line[start + 9..start + 9 + end];
                        files.push(ProjectFile {
                            path: file_path.to_string(),
                            filter: None, // Will be populated from filter file
                        });
                    }
                }
            }
        }
        
        Ok(files)
    }

    pub fn add_include_directory(&mut self, include_path: &str) -> Result<Vec<String>> {
        let mut lines: Vec<String> = self.content.lines().map(|s| s.to_string()).collect();
        let mut modified_configs = Vec::new();
        let mut i = 0;

        while i < lines.len() {
            // Look for ItemDefinitionGroup with Condition
            if lines[i].trim_start().starts_with("<ItemDefinitionGroup Condition=") {
                // Extract configuration name
                if let Some(condition_start) = lines[i].find("Condition=\"") {
                    if let Some(condition_end) = lines[i][condition_start + 11..].find('"') {
                        let condition = &lines[i][condition_start + 11..condition_start + 11 + condition_end];
                        modified_configs.push(condition.to_string());
                    }
                }

                // Look for ClCompile section within this ItemDefinitionGroup
                let mut j = i + 1;
                let mut found_clcompile = false;
                while j < lines.len() && !lines[j].trim().starts_with("</ItemDefinitionGroup>") {
                    if lines[j].trim_start().starts_with("<ClCompile>") {
                        found_clcompile = true;
                        // Look for existing AdditionalIncludeDirectories or find where to insert
                        let mut k = j + 1;
                        let mut found_includes = false;
                        while k < lines.len() && !lines[k].trim().starts_with("</ClCompile>") {
                            if lines[k].trim_start().starts_with("<AdditionalIncludeDirectories>") {
                                // Add to existing include directories
                                if lines[k].contains("%(AdditionalIncludeDirectories)") {
                                    lines[k] = lines[k].replace("%(AdditionalIncludeDirectories)", &format!("{};%(AdditionalIncludeDirectories)", include_path));
                                } else {
                                    lines[k] = lines[k].replace("</AdditionalIncludeDirectories>", &format!(";{}</AdditionalIncludeDirectories>", include_path));
                                }
                                found_includes = true;
                                break;
                            }
                            k += 1;
                        }
                        if !found_includes {
                            // Insert new AdditionalIncludeDirectories after ClCompile start
                            lines.insert(j + 1, format!("      <AdditionalIncludeDirectories>{};%(AdditionalIncludeDirectories)</AdditionalIncludeDirectories>", include_path));
                        }
                        break;
                    }
                    j += 1;
                }
                
                if !found_clcompile {
                    // Insert new ClCompile section with include directory
                    lines.insert(i + 1, format!("    <ClCompile>"));
                    lines.insert(i + 2, format!("      <AdditionalIncludeDirectories>{};%(AdditionalIncludeDirectories)</AdditionalIncludeDirectories>", include_path));
                    lines.insert(i + 3, format!("    </ClCompile>"));
                }
            }
            i += 1;
        }

        self.content = lines.join("\n");
        Ok(modified_configs)
    }

    pub fn add_library_directory(&mut self, lib_path: &str) -> Result<Vec<String>> {
        let mut lines: Vec<String> = self.content.lines().map(|s| s.to_string()).collect();
        let mut modified_configs = Vec::new();
        let mut i = 0;

        while i < lines.len() {
            // Look for ItemDefinitionGroup with Condition
            if lines[i].trim_start().starts_with("<ItemDefinitionGroup Condition=") {
                // Extract configuration name
                if let Some(condition_start) = lines[i].find("Condition=\"") {
                    if let Some(condition_end) = lines[i][condition_start + 11..].find('"') {
                        let condition = &lines[i][condition_start + 11..condition_start + 11 + condition_end];
                        modified_configs.push(condition.to_string());
                    }
                }

                // Look for Link section within this ItemDefinitionGroup
                let mut j = i + 1;
                let mut found_link = false;
                while j < lines.len() && !lines[j].trim().starts_with("</ItemDefinitionGroup>") {
                    if lines[j].trim_start().starts_with("<Link>") {
                        found_link = true;
                        // Look for existing AdditionalLibraryDirectories or find where to insert
                        let mut k = j + 1;
                        let mut found_lib_dirs = false;
                        while k < lines.len() && !lines[k].trim().starts_with("</Link>") {
                            if lines[k].trim_start().starts_with("<AdditionalLibraryDirectories>") {
                                // Add to existing library directories
                                if lines[k].contains("%(AdditionalLibraryDirectories)") {
                                    lines[k] = lines[k].replace("%(AdditionalLibraryDirectories)", &format!("{};%(AdditionalLibraryDirectories)", lib_path));
                                } else {
                                    lines[k] = lines[k].replace("</AdditionalLibraryDirectories>", &format!(";{}</AdditionalLibraryDirectories>", lib_path));
                                }
                                found_lib_dirs = true;
                                break;
                            }
                            k += 1;
                        }
                        if !found_lib_dirs {
                            // Insert new AdditionalLibraryDirectories after Link start
                            lines.insert(j + 1, format!("      <AdditionalLibraryDirectories>{};%(AdditionalLibraryDirectories)</AdditionalLibraryDirectories>", lib_path));
                        }
                        break;
                    }
                    j += 1;
                }
                
                if !found_link {
                    // Insert new Link section with library directory
                    lines.insert(i + 1, format!("    <Link>"));
                    lines.insert(i + 2, format!("      <AdditionalLibraryDirectories>{};%(AdditionalLibraryDirectories)</AdditionalLibraryDirectories>", lib_path));
                    lines.insert(i + 3, format!("    </Link>"));
                }
            }
            i += 1;
        }

        self.content = lines.join("\n");
        Ok(modified_configs)
    }

    pub fn add_library_dependency(&mut self, lib_name: &str) -> Result<Vec<String>> {
        let mut lines: Vec<String> = self.content.lines().map(|s| s.to_string()).collect();
        let mut modified_configs = Vec::new();
        let mut i = 0;

        while i < lines.len() {
            // Look for ItemDefinitionGroup with Condition
            if lines[i].trim_start().starts_with("<ItemDefinitionGroup Condition=") {
                // Extract configuration name
                if let Some(condition_start) = lines[i].find("Condition=\"") {
                    if let Some(condition_end) = lines[i][condition_start + 11..].find('"') {
                        let condition = &lines[i][condition_start + 11..condition_start + 11 + condition_end];
                        modified_configs.push(condition.to_string());
                    }
                }

                // Look for Link section within this ItemDefinitionGroup
                let mut j = i + 1;
                let mut found_link = false;
                while j < lines.len() && !lines[j].trim().starts_with("</ItemDefinitionGroup>") {
                    if lines[j].trim_start().starts_with("<Link>") {
                        found_link = true;
                        // Look for existing AdditionalDependencies or find where to insert
                        let mut k = j + 1;
                        let mut found_deps = false;
                        while k < lines.len() && !lines[k].trim().starts_with("</Link>") {
                            if lines[k].trim_start().starts_with("<AdditionalDependencies>") {
                                // Add to existing dependencies
                                if lines[k].contains("%(AdditionalDependencies)") {
                                    lines[k] = lines[k].replace("%(AdditionalDependencies)", &format!("{};%(AdditionalDependencies)", lib_name));
                                } else {
                                    lines[k] = lines[k].replace("</AdditionalDependencies>", &format!(";{}</AdditionalDependencies>", lib_name));
                                }
                                found_deps = true;
                                break;
                            }
                            k += 1;
                        }
                        if !found_deps {
                            // Insert new AdditionalDependencies after Link start
                            lines.insert(j + 1, format!("      <AdditionalDependencies>{};%(AdditionalDependencies)</AdditionalDependencies>", lib_name));
                        }
                        break;
                    }
                    j += 1;
                }
                
                if !found_link {
                    // Insert new Link section with library dependency
                    lines.insert(i + 1, format!("    <Link>"));
                    lines.insert(i + 2, format!("      <AdditionalDependencies>{};%(AdditionalDependencies)</AdditionalDependencies>", lib_name));
                    lines.insert(i + 3, format!("    </Link>"));
                }
            }
            i += 1;
        }

        self.content = lines.join("\n");
        Ok(modified_configs)
    }

    pub fn save(&self) -> Result<()> {
        fs::write(&self.path, &self.content)
            .with_context(|| format!("Failed to write vcxproj file: {}", self.path.display()))?;
        Ok(())
    }
}

impl FilterFile {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read filters file: {}", path.display()))?;
        
        Ok(Self { path, content })
    }

    pub fn add_source_files(&mut self, files: &[PathBuf]) -> Result<()> {
        // Collect unique directories for filters
        let mut dirs = HashSet::new();
        for file in files {
            if let Some(parent) = file.parent() {
                let filter_name = parent.to_string_lossy().replace('/', "\\");
                if !filter_name.is_empty() {
                    dirs.insert(filter_name);
                }
            }
        }

        // Add filter entries
        let mut new_filters = String::new();
        for dir in &dirs {
            let uuid = uuid::Uuid::new_v4();
            new_filters.push_str(&format!(
                "    <Filter Include=\"{}\">\n      <UniqueIdentifier>{{{}}}</UniqueIdentifier>\n    </Filter>\n",
                dir, uuid.to_string().to_uppercase()
            ));
        }

        // Add ClCompile entries
        let mut new_clcompile = String::new();
        for file in files {
            if let Some(ext) = file.extension() {
                if ext == "c" || ext == "cpp" || ext == "cc" || ext == "cxx" {
                    let include_path = file.to_string_lossy().replace('/', "\\");
                    new_clcompile.push_str(&format!("    <ClCompile Include=\"{}\">\n", include_path));
                    
                    if let Some(parent) = file.parent() {
                        let filter_name = parent.to_string_lossy().replace('/', "\\");
                        if !filter_name.is_empty() {
                            new_clcompile.push_str(&format!("      <Filter>{}</Filter>\n", filter_name));
                        } else {
                            new_clcompile.push_str("      <Filter>Source Files</Filter>\n");
                        }
                    } else {
                        new_clcompile.push_str("      <Filter>Source Files</Filter>\n");
                    }
                    
                    new_clcompile.push_str("    </ClCompile>\n");
                }
            }
        }

        // Insert filters if we have new ones
        if !new_filters.is_empty() {
            if let Some(pos) = self.content.find("<Filter Include=") {
                // Find the ItemGroup containing filters
                let before_pos = &self.content[..pos];
                if let Some(itemgroup_start) = before_pos.rfind("<ItemGroup>") {
                    let after_itemgroup = &self.content[itemgroup_start..];
                    if let Some(itemgroup_end) = after_itemgroup.find("</ItemGroup>") {
                        let insertion_point = itemgroup_start + itemgroup_end;
                        self.content.insert_str(insertion_point, &new_filters);
                    }
                }
            } else {
                // Create new filter ItemGroup
                if let Some(pos) = self.content.find("  </ItemGroup>") {
                    let itemgroup = format!(
                        "  <ItemGroup>\n{}\n  </ItemGroup>\n",
                        new_filters.trim_end()
                    );
                    self.content.insert_str(pos, &itemgroup);
                }
            }
        }

        // Insert ClCompile entries
        if !new_clcompile.is_empty() {
            if let Some(pos) = self.content.find("<ClCompile Include=") {
                // Find the ItemGroup containing ClCompile
                let before_pos = &self.content[..pos];
                if let Some(itemgroup_start) = before_pos.rfind("<ItemGroup>") {
                    let after_itemgroup = &self.content[itemgroup_start..];
                    if let Some(itemgroup_end) = after_itemgroup.find("</ItemGroup>") {
                        let insertion_point = itemgroup_start + itemgroup_end;
                        self.content.insert_str(insertion_point, &new_clcompile);
                    }
                }
            } else {
                // Create new ClCompile ItemGroup before closing Project
                if let Some(pos) = self.content.rfind("</Project>") {
                    let itemgroup = format!(
                        "  <ItemGroup>\n{}\n  </ItemGroup>\n",
                        new_clcompile.trim_end()
                    );
                    self.content.insert_str(pos, &itemgroup);
                }
            }
        }

        Ok(())
    }

    pub fn delete_files_and_filters(&mut self, target: &str, extension: Option<&str>) -> Result<(Vec<String>, Vec<String>)> {
        let mut deleted_files = Vec::new();
        let mut deleted_filters = Vec::new();
        let mut lines: Vec<String> = self.content.lines().map(|s| s.to_string()).collect();
        let mut filters_to_delete = HashSet::new();
        
        // First pass: delete ClCompile entries and collect filters that might need deletion
        let mut i = 0;
        while i < lines.len() {
            let line = &lines[i];
            
            if line.trim_start().starts_with("<ClCompile Include=\"") {
                let should_delete = if let Some(ext) = extension {
                    // Delete by extension
                    line.contains(&format!(".{}", ext))
                } else {
                    // Delete by specific file path or folder
                    if target.ends_with('/') || target.ends_with('\\') {
                        // Folder deletion - check if file is in this folder
                        let target_normalized = target.replace('/', "\\");
                        line.contains(&target_normalized) || line.contains(&target.replace('\\', "/"))
                    } else {
                        // Specific file deletion
                        line.contains(target)
                    }
                };
                
                if should_delete {
                    // Extract filename for reporting
                    if let Some(start) = line.find("Include=\"") {
                        if let Some(end) = line[start + 9..].find('"') {
                            let filename = &line[start + 9..start + 9 + end];
                            deleted_files.push(filename.to_string());
                        }
                    }
                    
                    // Find the filter for this file to potentially delete later
                    let mut j = i + 1;
                    while j < lines.len() && !lines[j].trim().starts_with("</ClCompile>") {
                        if lines[j].trim_start().starts_with("<Filter>") {
                            if let Some(filter_start) = lines[j].find("<Filter>") {
                                if let Some(filter_end) = lines[j].find("</Filter>") {
                                    let filter_name = &lines[j][filter_start + 8..filter_end];
                                    filters_to_delete.insert(filter_name.to_string());
                                }
                            }
                        }
                        j += 1;
                    }
                    
                    // Remove the ClCompile entry
                    lines.remove(i);
                    while i < lines.len() && !lines[i].trim().ends_with("</ClCompile>") {
                        lines.remove(i);
                    }
                    if i < lines.len() {
                        lines.remove(i); // Remove closing tag
                    }
                } else {
                    i += 1;
                }
            } else {
                i += 1;
            }
        }
        
        // Handle direct filter deletion (e.g., "Header Files")
        let is_filter_deletion = !target.contains('.') && !target.contains('/') && !target.contains('\\') && extension.is_none();
        if is_filter_deletion {
            filters_to_delete.insert(target.to_string());
            
            // Also delete all files in this filter
            let mut i = 0;
            while i < lines.len() {
                let line = &lines[i];
                
                if line.trim_start().starts_with("<ClCompile Include=\"") {
                    let mut j = i + 1;
                    let mut file_in_filter = false;
                    
                    while j < lines.len() && !lines[j].trim().starts_with("</ClCompile>") {
                        if lines[j].trim_start().starts_with("<Filter>") {
                            if lines[j].contains(&format!(">{}<", target)) {
                                file_in_filter = true;
                                
                                // Extract filename for reporting
                                if let Some(start) = line.find("Include=\"") {
                                    if let Some(end) = line[start + 9..].find('"') {
                                        let filename = &line[start + 9..start + 9 + end];
                                        deleted_files.push(filename.to_string());
                                    }
                                }
                                break;
                            }
                        }
                        j += 1;
                    }
                    
                    if file_in_filter {
                        // Remove the ClCompile entry
                        lines.remove(i);
                        while i < lines.len() && !lines[i].trim().ends_with("</ClCompile>") {
                            lines.remove(i);
                        }
                        if i < lines.len() {
                            lines.remove(i); // Remove closing tag
                        }
                    } else {
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
        }
        
        // Second pass: delete empty filters or specifically targeted filters
        let mut i = 0;
        while i < lines.len() {
            let line = &lines[i];
            
            if line.trim_start().starts_with("<Filter Include=\"") {
                // Extract filter name
                if let Some(start) = line.find("Include=\"") {
                    if let Some(end) = line[start + 9..].find('"') {
                        let filter_name = &line[start + 9..start + 9 + end];
                        
                        // Check if this filter should be deleted
                        let should_delete_filter = filters_to_delete.contains(filter_name) || 
                            (is_filter_deletion && filter_name == target) ||
                            !self.filter_has_files(&lines, filter_name);
                        
                        if should_delete_filter {
                            deleted_filters.push(filter_name.to_string());
                            
                            // Remove the filter entry
                            if line.trim().ends_with("/>") {
                                // Self-closing tag
                                lines.remove(i);
                            } else {
                                // Multi-line entry, find the closing tag
                                lines.remove(i);
                                while i < lines.len() && !lines[i].trim().ends_with("</Filter>") {
                                    lines.remove(i);
                                }
                                if i < lines.len() {
                                    lines.remove(i); // Remove closing tag
                                }
                            }
                        } else {
                            i += 1;
                        }
                    } else {
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            } else {
                i += 1;
            }
        }
        
        self.content = lines.join("\n");
        Ok((deleted_files, deleted_filters))
    }
    
    fn filter_has_files(&self, lines: &[String], filter_name: &str) -> bool {
        for line in lines {
            if line.trim_start().starts_with("<ClCompile Include=\"") {
                // Look ahead for filter tag
                let line_index = lines.iter().position(|l| l == line).unwrap_or(0);
                for j in (line_index + 1)..lines.len() {
                    if lines[j].trim().starts_with("</ClCompile>") {
                        break;
                    }
                    if lines[j].trim_start().starts_with("<Filter>") {
                        if lines[j].contains(&format!(">{}<", filter_name)) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    pub fn get_file_filters(&self) -> Result<HashMap<String, String>> {
        let mut file_to_filter = HashMap::new();
        let lines: Vec<&str> = self.content.lines().collect();
        let mut i = 0;
        
        while i < lines.len() {
            let line = lines[i].trim_start();
            if line.starts_with("<ClCompile Include=\"") {
                if let Some(start) = lines[i].find("Include=\"") {
                    if let Some(end) = lines[i][start + 9..].find('"') {
                        let file_path = &lines[i][start + 9..start + 9 + end];
                        
                        // Look for the filter in subsequent lines
                        let mut j = i + 1;
                        while j < lines.len() && !lines[j].trim().starts_with("</ClCompile>") {
                            if lines[j].trim_start().starts_with("<Filter>") {
                                if let Some(filter_start) = lines[j].find("<Filter>") {
                                    if let Some(filter_end) = lines[j].find("</Filter>") {
                                        let filter_name = &lines[j][filter_start + 8..filter_end];
                                        file_to_filter.insert(file_path.to_string(), filter_name.to_string());
                                        break;
                                    }
                                }
                            }
                            j += 1;
                        }
                    }
                }
            }
            i += 1;
        }
        
        Ok(file_to_filter)
    }
    
    pub fn get_all_filters(&self) -> Result<HashMap<String, Vec<String>>> {
        let mut filters = HashMap::new();
        let lines: Vec<&str> = self.content.lines().collect();
        
        // First, collect all filter names
        for line in &lines {
            let trimmed = line.trim_start();
            if trimmed.starts_with("<Filter Include=\"") {
                if let Some(start) = line.find("Include=\"") {
                    if let Some(end) = line[start + 9..].find('"') {
                        let filter_name = &line[start + 9..start + 9 + end];
                        filters.insert(filter_name.to_string(), Vec::new());
                    }
                }
            }
        }
        
        // Then, map files to their filters
        let file_filters = self.get_file_filters()?;
        for (file, filter) in file_filters {
            if let Some(files) = filters.get_mut(&filter) {
                files.push(file);
            }
        }
        
        Ok(filters)
    }

    pub fn rename_filter(&mut self, from: &str, to: &str) -> Result<(bool, Vec<String>)> {
        let mut lines: Vec<String> = self.content.lines().map(|s| s.to_string()).collect();
        let mut renamed_files = Vec::new();
        let mut filter_exists = false;
        let mut target_filter_exists = false;
        
        // First pass: check if filters exist
        for line in &lines {
            let trimmed = line.trim_start();
            if trimmed.starts_with("<Filter Include=\"") {
                if let Some(start) = line.find("Include=\"") {
                    if let Some(end) = line[start + 9..].find('"') {
                        let filter_name = &line[start + 9..start + 9 + end];
                        if filter_name == from {
                            filter_exists = true;
                        }
                        if filter_name == to {
                            target_filter_exists = true;
                        }
                    }
                }
            }
        }
        
        if !filter_exists {
            return Err(anyhow::anyhow!("Filter '{}' not found in project", from));
        }
        
        // Second pass: rename filter definition and file assignments
        for i in 0..lines.len() {
            let line_copy = lines[i].clone();
            let trimmed = line_copy.trim_start();
            
            // Rename filter definition
            if trimmed.starts_with("<Filter Include=\"") {
                if let Some(start) = line_copy.find("Include=\"") {
                    if let Some(end) = line_copy[start + 9..].find('"') {
                        let filter_name = &line_copy[start + 9..start + 9 + end];
                        if filter_name == from {
                            lines[i] = line_copy.replace(&format!("Include=\"{}\"", from), &format!("Include=\"{}\"", to));
                        }
                    }
                }
            }
            
            // Rename filter assignments in ClCompile entries
            if trimmed.starts_with("<Filter>") && trimmed.ends_with("</Filter>") {
                if let Some(filter_start) = line_copy.find("<Filter>") {
                    if let Some(filter_end) = line_copy.find("</Filter>") {
                        let filter_name = &line_copy[filter_start + 8..filter_end];
                        if filter_name == from {
                            lines[i] = line_copy.replace(&format!(">{}<", from), &format!(">{}<", to));
                        }
                    }
                }
            }
        }
        
        // Collect files that were moved
        let mut i = 0;
        while i < lines.len() {
            let line = &lines[i];
            if line.trim_start().starts_with("<ClCompile Include=\"") {
                if let Some(start) = line.find("Include=\"") {
                    if let Some(end) = line[start + 9..].find('"') {
                        let file_path = &line[start + 9..start + 9 + end];
                        
                        // Look for the filter in subsequent lines
                        let mut j = i + 1;
                        while j < lines.len() && !lines[j].trim().starts_with("</ClCompile>") {
                            if lines[j].contains(&format!(">{}<", to)) {
                                renamed_files.push(file_path.to_string());
                                break;
                            }
                            j += 1;
                        }
                    }
                }
            }
            i += 1;
        }
        
        self.content = lines.join("\n");
        Ok((target_filter_exists, renamed_files))
    }
    
    pub fn merge_filters(&mut self, from: &str, to: &str) -> Result<Vec<String>> {
        let mut lines: Vec<String> = self.content.lines().map(|s| s.to_string()).collect();
        let mut moved_files = Vec::new();
        
        // First pass: Move all files from 'from' filter to 'to' filter
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i].clone();
            if line.trim_start().starts_with("<ClCompile Include=\"") {
                if let Some(start) = line.find("Include=\"") {
                    if let Some(end) = line[start + 9..].find('"') {
                        let file_path = line[start + 9..start + 9 + end].to_string();
                        
                        // Look for the filter in subsequent lines
                        let mut j = i + 1;
                        while j < lines.len() && !lines[j].trim().starts_with("</ClCompile>") {
                            if lines[j].contains(&format!(">{}<", from)) {
                                let new_line = lines[j].replace(&format!(">{}<", from), &format!(">{}<", to));
                                lines[j] = new_line;
                                moved_files.push(file_path);
                                break;
                            }
                            j += 1;
                        }
                    }
                }
            }
            i += 1;
        }
        
        // Second pass: Remove the empty 'from' filter definition
        let mut i = 0;
        while i < lines.len() {
            let line = &lines[i];
            if line.trim_start().starts_with("<Filter Include=\"") {
                if let Some(start) = line.find("Include=\"") {
                    if let Some(end) = line[start + 9..].find('"') {
                        let filter_name = &line[start + 9..start + 9 + end];
                        if filter_name == from {
                            // Remove the filter definition
                            if line.trim().ends_with("/>") {
                                // Self-closing tag
                                lines.remove(i);
                            } else {
                                // Multi-line entry, find the closing tag
                                lines.remove(i);
                                while i < lines.len() && !lines[i].trim().ends_with("</Filter>") {
                                    lines.remove(i);
                                }
                                if i < lines.len() {
                                    lines.remove(i); // Remove closing tag
                                }
                            }
                            break;
                        }
                    }
                }
            }
            i += 1;
        }
        
        self.content = lines.join("\n");
        Ok(moved_files)
    }

    pub fn save(&self) -> Result<()> {
        fs::write(&self.path, &self.content)
            .with_context(|| format!("Failed to write filters file: {}", self.path.display()))?;
        Ok(())
    }
}

impl ProjectStructure {
    pub fn from_project(vcxproj_path: &Path) -> Result<Self> {
        let vcxproj = VcxprojFile::load(vcxproj_path)?;
        let mut files = vcxproj.get_project_files()?;
        
        let project_name = vcxproj_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        
        // Try to load filter file
        let filter_path = vcxproj_path.with_extension("vcxproj.filters");
        let (filters, file_filters) = if filter_path.exists() {
            let filter_file = FilterFile::load(&filter_path)?;
            let filters = filter_file.get_all_filters()?;
            let file_filters = filter_file.get_file_filters()?;
            (filters, file_filters)
        } else {
            (HashMap::new(), HashMap::new())
        };
        
        // Update files with their filter information
        for file in &mut files {
            file.filter = file_filters.get(&file.path).cloned();
        }
        
        Ok(ProjectStructure {
            name: project_name,
            files,
            filters,
        })
    }
    
    pub fn display_tree(&self, files_only: bool, _show_extensions: bool, level: Option<usize>) -> String {
        let mut output = String::new();
        
        // Project root - always show extension
        let project_display = format!("{}.vcxproj", self.name);
        output.push_str(&format!("📁 {}\n", project_display));
        
        if self.files.is_empty() && self.filters.is_empty() {
            output.push_str("   (empty project)\n");
            return output;
        }
        
        // Group files by filter
        let mut filter_files: HashMap<String, Vec<&ProjectFile>> = HashMap::new();
        let mut unfiltered_files = Vec::new();
        
        for file in &self.files {
            if let Some(filter) = &file.filter {
                filter_files.entry(filter.clone()).or_default().push(file);
            } else {
                unfiltered_files.push(file);
            }
        }
        
        // Create a structure of all items with their depth levels
        let mut items: Vec<(String, usize, bool, Vec<&ProjectFile>)> = Vec::new(); // (name, depth, is_filter, files)
        
        // Add filters
        let mut filter_names: Vec<_> = filter_files.keys().cloned().collect();
        // Add existing empty filters
        for filter_name in self.filters.keys() {
            if !filter_names.contains(filter_name) {
                filter_names.push(filter_name.clone());
            }
        }
        filter_names.sort();
        
        for filter_name in &filter_names {
            let depth = filter_name.matches('\\').count() + 1; // Count directory separators + 1
            let files = filter_files.get(filter_name).cloned().unwrap_or_default();
            items.push((filter_name.clone(), depth, true, files));
        }
        
        // Add unfiltered files as "Source Files" at depth 1
        if !unfiltered_files.is_empty() {
            items.push(("Source Files".to_string(), 1, true, unfiltered_files));
        }
        
        // Sort by depth first, then by name
        items.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        
        // Display items based on level restrictions
        self.display_items(&mut output, &items, level, files_only);
        
        output
    }
    
    fn display_items(
        &self, 
        output: &mut String, 
        items: &[(String, usize, bool, Vec<&ProjectFile>)], 
        level: Option<usize>,
        files_only: bool
    ) {
        let mut processed_filters = HashSet::new();
        let mut current_prefix_stack: Vec<(String, bool)> = Vec::new(); // (prefix, is_last)
        
        for (index, (item_name, depth, is_filter, files)) in items.iter().enumerate() {
            // Check if we should display this item based on level
            if let Some(max_level) = level {
                if *depth > max_level {
                    continue;
                }
            }
            
            if *is_filter {
                if processed_filters.contains(item_name) {
                    continue;
                }
                processed_filters.insert(item_name.clone());
                
                if files.is_empty() && files_only {
                    continue; // Skip empty filters if files_only is true
                }
                
                // Adjust prefix stack to current depth
                current_prefix_stack.truncate(*depth - 1);
                
                // Determine if this is the last item at this depth
                let is_last_at_depth = !items.iter().skip(index + 1).any(|(_, d, _, _)| d == depth);
                
                // Build prefix for this filter
                let prefix = self.build_prefix(&current_prefix_stack);
                let current_symbol = if is_last_at_depth { "└── " } else { "├── " };
                
                output.push_str(&format!("{}{}📁 {}\n", prefix, current_symbol, item_name));
                
                // Update prefix stack for children
                current_prefix_stack.push((
                    if is_last_at_depth { "    " } else { "│   " }.to_string(),
                    is_last_at_depth
                ));
                
                // Show files in this filter if level allows
                if let Some(max_level) = level {
                    if max_level == 0 {
                        continue; // Level 0 = folders only
                    }
                    if *depth + 1 > max_level {
                        continue; // Files would exceed max level
                    }
                }
                
                let mut sorted_files = files.clone();
                sorted_files.sort_by_key(|f| &f.path);
                
                for (file_index, file) in sorted_files.iter().enumerate() {
                    let is_last_file = file_index == sorted_files.len() - 1;
                    let file_prefix = if is_last_file { "└── " } else { "├── " };
                    
                    let file_display = Path::new(&file.path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    
                    let current_prefix = self.build_prefix(&current_prefix_stack);
                    output.push_str(&format!("{}{}📄 {}\n", current_prefix, file_prefix, file_display));
                }
            }
        }
    }
    
    fn build_prefix(&self, stack: &[(String, bool)]) -> String {
        stack.iter().map(|(prefix, _)| prefix.as_str()).collect::<String>()
    }
}