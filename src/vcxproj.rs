use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet, BTreeMap};
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


    pub fn add_source_files_with_hierarchy(&mut self, project_files: &[PathBuf], scan_relative_files: &[PathBuf]) -> Result<()> {
        // Collect unique directories for filters using scan_relative_files for hierarchy
        let mut dirs = HashSet::new();
        for file in scan_relative_files {
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

        // Add ClCompile entries using project_files for Include paths and scan_relative_files for Filter assignments
        let mut new_clcompile = String::new();
        for (i, project_file) in project_files.iter().enumerate() {
            let scan_relative_file = &scan_relative_files[i];
            if let Some(ext) = project_file.extension() {
                if ext == "c" || ext == "cpp" || ext == "cc" || ext == "cxx" {
                    let include_path = project_file.to_string_lossy().replace('/', "\\");
                    new_clcompile.push_str(&format!("    <ClCompile Include=\"{}\">\n", include_path));
                    
                    if let Some(parent) = scan_relative_file.parent() {
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
                        
                        // Check if this is a self-closing tag
                        if lines[i].trim().ends_with("/>") {
                            // Self-closing tag, no filter - skip
                        } else {
                            // Look for the filter in subsequent lines until we find </ClCompile>
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
        
        // Build hierarchical tree structure
        self.display_hierarchical_tree(&mut output, &filter_files, &unfiltered_files, level, files_only);
        
        output
    }
    
    fn display_hierarchical_tree(
        &self,
        output: &mut String,
        filter_files: &HashMap<String, Vec<&ProjectFile>>,
        unfiltered_files: &[&ProjectFile],
        level: Option<usize>,
        files_only: bool,
    ) {
        // Build a simple hierarchical structure
        use std::collections::BTreeMap;
        
        // Create a sorted list of all filters (existing and empty)
        let mut all_filters: Vec<String> = filter_files.keys().cloned().collect();
        for filter_name in self.filters.keys() {
            if !all_filters.contains(filter_name) {
                all_filters.push(filter_name.clone());
            }
        }
        all_filters.sort();
        
        // Build a tree structure for filters
        let mut filter_tree: BTreeMap<String, Vec<String>> = BTreeMap::new(); // parent -> children
        let mut filter_files_map: HashMap<String, Vec<&ProjectFile>> = HashMap::new();
        
        // First pass: identify all parent-child relationships
        for filter in &all_filters {
            let parts: Vec<&str> = filter.split('\\').collect();
            
            if parts.len() == 1 {
                // Root level filter
                filter_tree.entry(String::new()).or_default().push(filter.clone());
            } else {
                // Child filter - find its parent
                let parent = parts[..parts.len()-1].join("\\");
                filter_tree.entry(parent).or_default().push(filter.clone());
            }
            
            // Store files for this filter
            if let Some(files) = filter_files.get(filter) {
                filter_files_map.insert(filter.clone(), files.clone());
            }
        }
        
        // Display unfiltered files first at root level (unless level=0 which means folders only)
        let show_root_files = level.map_or(true, |l| l > 0);
        let unfiltered_count = if show_root_files { unfiltered_files.len() } else { 0 };
        let total_root_items = unfiltered_count + filter_tree.get("").map_or(0, |v| v.len());
        let mut current_index = 0;
        
        if show_root_files {
            for file in unfiltered_files {
                let is_last = current_index == total_root_items - 1;
                let symbol = if is_last { "└── " } else { "├── " };
                let file_name = std::path::Path::new(&file.path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                output.push_str(&format!("{}📄 {}\n", symbol, file_name));
                current_index += 1;
            }
        }
        
        // Display root level filters
        if let Some(root_filters) = filter_tree.get("") {
            for filter_name in root_filters {
                let is_last = current_index == total_root_items - 1;
                self.display_filter_recursive(
                    output,
                    filter_name,
                    &filter_tree,
                    &filter_files_map,
                    "",
                    is_last,
                    1,
                    level,
                    files_only,
                );
                current_index += 1;
            }
        }
    }
    
    fn display_filter_recursive(
        &self,
        output: &mut String,
        filter_name: &str,
        filter_tree: &BTreeMap<String, Vec<String>>,
        filter_files_map: &HashMap<String, Vec<&ProjectFile>>,
        prefix: &str,
        is_last: bool,
        depth: usize,
        max_level: Option<usize>,
        files_only: bool,
    ) {
        // Check level restriction for folders
        // For level 0, we show all folders but no files
        // For level N (N>0), we show folders and files up to depth N
        if let Some(max) = max_level {
            if max == 0 {
                // Level 0: show folders but no files (files are handled separately)
                // Continue to show the folder
            } else if depth > max {
                // Level N: don't show folders beyond depth N
                return;
            }
        }
        
        // Get files for this filter
        let files = filter_files_map.get(filter_name).cloned().unwrap_or_default();
        let children = filter_tree.get(filter_name).cloned().unwrap_or_default();
        
        // Skip empty filters if files_only is true
        if files_only && files.is_empty() && children.is_empty() {
            return;
        }
        
        // Display this filter
        let symbol = if is_last { "└── " } else { "├── " };
        let display_name = if filter_name.contains('\\') {
            filter_name.split('\\').last().unwrap()
        } else {
            filter_name
        };
        output.push_str(&format!("{}{}📁 {}\n", prefix, symbol, display_name));
        
        // Prepare prefix for children
        let child_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
        
        // Display children (sub-filters and files)
        let total_children = children.len() + files.len();
        let mut child_index = 0;
        
        // Display child filters first
        for child_filter in &children {
            let is_last_child = child_index == total_children - 1;
            self.display_filter_recursive(
                output,
                child_filter,
                filter_tree,
                filter_files_map,
                &child_prefix,
                is_last_child,
                depth + 1,
                max_level,
                files_only,
            );
            child_index += 1;
        }
        
        // Display files in this filter (only if level allows and level > 0)
        // Level 0 means folders only, so no files should be shown
        // Files are considered to be at depth + 1 relative to their containing folder
        let file_depth = depth + 1;
        let show_files = max_level.map_or(true, |max| max > 0 && file_depth <= max);
        if show_files {
            let mut sorted_files = files;
            sorted_files.sort_by_key(|f| &f.path);
            
            for file in &sorted_files {
                let is_last_file = child_index == total_children - 1;
                let file_symbol = if is_last_file { "└── " } else { "├── " };
                
                let file_name = std::path::Path::new(&file.path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                
                output.push_str(&format!("{}{}📄 {}\n", child_prefix, file_symbol, file_name));
                child_index += 1;
            }
        }
    }
    
}