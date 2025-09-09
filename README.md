# Visual Studio Project Manager

A Rust command-line tool for manipulating Visual Studio project files (.vcxproj) and their associated filter files (.vcxproj.filters).

## Features

- **Add source files**: Automatically adds all files of a specified extension to a Visual Studio project
- **Delete files and folders**: Remove files, entire folders, or all files of a specific extension from the project
- **View project structure**: Display project structure as it appears in Visual Studio with tree-like visualization
- **Rename folders/filters**: Change folder names with conflict detection and merge capabilities
- **Folder structure preservation**: Maintains folder structure in both .vcxproj and .vcxproj.filters files
- **Recursive scanning**: Optionally scans subdirectories for files
- **Filter management**: Updates or creates .vcxproj.filters files with proper folder organization
- **Interactive confirmation**: Preview changes before applying them
- **Cross-platform**: Works on Windows, macOS, and Linux

## Installation

1. Ensure you have Rust installed on your system
2. Clone this repository
3. Build the project:
   ```bash
   cargo build --release
   ```
4. The executable will be available at `target/release/vs-project-manager` (or `vs-project-manager.exe` on Windows)

## Usage

### Add Files to Project

Add all files of a specific extension to a Visual Studio project:

```bash
# Add all .c files from the project directory
vs-project-manager add --extension c --project path/to/project.vcxproj

# Add all .cpp files using short aliases
vs-project-manager a -e cpp -p path/to/project.vcxproj

# Scan a specific directory instead of the project directory
vs-project-manager add -e c -p project.vcxproj -d src/

# Disable recursive scanning (only scan the root directory)
vs-project-manager add -e c -p project.vcxproj --recursive false
```

### Command Options

- `-e, --extension <EXTENSION>`: File extension to add (e.g., "c", "cpp", "cc", "cxx")
- `-p, --project <PROJECT>`: Path to the .vcxproj file
- `-d, --directory <DIRECTORY>`: Root directory to scan for files (defaults to project directory)
- `-r, --recursive`: Include subdirectories in scan (default: true)

### Delete Files from Project

Remove files, folders, or all files of a specific extension from the project:

```bash
# Delete a specific file
vs-project-manager delete --target "main.c" --project MyProject.vcxproj

# Delete an entire folder and its contents
vs-project-manager del -t "src/utils" -p MyProject.vcxproj

# Delete an entire filter (e.g., "Header Files")
vs-project-manager del -t "Header Files" -p MyProject.vcxproj

# Delete all files by extension
vs-project-manager del -e c -p MyProject.vcxproj

# Skip confirmation prompt
vs-project-manager del -t "main.c" -p MyProject.vcxproj -y
```

### Delete Command Options

- `-p, --project <PROJECT>`: Path to the .vcxproj file
- `-t, --target <TARGET>`: Filter name or file path to delete (e.g., "Header Files", "src/utils", "main.c")
- `-e, --extension <EXTENSION>`: Delete by file extension instead of specific path
- `-y, --yes`: Confirm deletion without prompting

### View Project Structure

Display the project structure as it appears in Visual Studio:

```bash
# Basic view (file extensions always shown)
vs-project-manager view --project MyProject.vcxproj

# Show only files (hide empty filters)
vs-project-manager v -p MyProject.vcxproj --files-only
```

### View Command Options

- `-p, --project <PROJECT>`: Path to the .vcxproj file
- `-f, --files-only`: Show only files (don't show empty filters)

**Note**: File extensions are always displayed in the view output.

### Rename Folders/Filters

Rename folders and filters in the project structure:

```bash
# Basic rename
vs-project-manager rename --from "old_name" --to "new_name" --project MyProject.vcxproj

# Using short alias with auto-confirm
vs-project-manager ren -f "src" -t "source" -p MyProject.vcxproj -y

# Rename nested folder
vs-project-manager rename -f "src\\utils" -t "utilities" -p MyProject.vcxproj
```

### Rename Command Options

- `-p, --project <PROJECT>`: Path to the .vcxproj file
- `-f, --from <FROM>`: Current folder/filter name to rename
- `-t, --to <TO>`: New folder/filter name
- `-y, --yes`: Skip confirmation prompts

**Note**: If the target folder already exists, the tool will warn you and ask if you want to merge the folders.

### Examples

#### Adding Files

1. **Add all C files to a project:**
   ```bash
   vs-project-manager add -e c -p MyProject.vcxproj
   ```

2. **Add C++ files from a specific source directory:**
   ```bash
   vs-project-manager add -e cpp -p MyProject.vcxproj -d src/
   ```

3. **Add files non-recursively:**
   ```bash
   vs-project-manager a -e c -p MyProject.vcxproj -r false
   ```

#### Deleting Files

1. **Delete a specific file:**
   ```bash
   vs-project-manager del -t "main.c" -p MyProject.vcxproj
   ```

2. **Delete an entire source folder:**
   ```bash
   vs-project-manager del -t "src" -p MyProject.vcxproj
   ```

3. **Delete all header files (.h):**
   ```bash
   vs-project-manager del -e h -p MyProject.vcxproj
   ```

4. **Delete a Visual Studio filter:**
   ```bash
   vs-project-manager del -t "Header Files" -p MyProject.vcxproj
   ```

#### Viewing Project Structure

1. **View project structure:**
   ```bash
   vs-project-manager view -p MyProject.vcxproj
   ```

2. **View only files (no empty filters):**
   ```bash
   vs-project-manager v -p MyProject.vcxproj --files-only
   ```

#### Renaming Folders

1. **Rename a folder:**
   ```bash
   vs-project-manager rename -f "old_folder" -t "new_folder" -p MyProject.vcxproj
   ```

2. **Rename with auto-confirm:**
   ```bash
   vs-project-manager ren -f "src" -t "source" -p MyProject.vcxproj -y
   ```

3. **Merge conflicting folders:**
   ```bash
   # If "utilities" already exists, will prompt to merge
   vs-project-manager rename -f "utils" -t "utilities" -p MyProject.vcxproj
   ```

## What the tool does

### Project File (.vcxproj)
- **Add**: Adds `<ClCompile Include="..."/>` entries for each source file
- **Delete**: Removes `<ClCompile Include="..."/>` entries matching the specified criteria
- **View**: Parses and extracts all source files from the project
- **Rename**: File paths remain unchanged (renaming only affects filter organization)
- Creates or updates ItemGroup sections as needed
- Preserves all existing project settings and configurations

### Filter File (.vcxproj.filters)
- **Add**: Creates filter entries for each unique directory path and assigns files to filters
- **Delete**: Removes files from filters and deletes empty filters automatically
- **View**: Parses filter structure and file-to-filter mappings for visualization
- **Rename**: Updates filter names and reassigns files to new filter names
- Generates unique GUIDs for new filters
- Creates the file if it doesn't exist during add operations
- Handles both individual files and entire folder structures
- Supports merging folders when conflicts occur

### Delete Operations
- **File deletion**: Removes specific files from both .vcxproj and .vcxproj.filters
- **Folder deletion**: Removes entire folder structures and all contained files
- **Filter deletion**: Removes Visual Studio filter categories (e.g., "Header Files") and all their files
- **Extension deletion**: Removes all files with a specific extension (e.g., all .c files)
- **Auto-cleanup**: Automatically removes empty filters after file deletion
- **Preview mode**: Shows what will be deleted before making changes

### Rename Operations
- **Filter renaming**: Changes filter/folder names in the Visual Studio project structure
- **Conflict detection**: Automatically detects when target filter already exists
- **Interactive merging**: Prompts user to confirm folder merges when conflicts occur
- **File reassignment**: Moves files from old filter to new filter automatically
- **Cleanup**: Removes empty source filters after merge operations

### View Operations
- **Tree visualization**: Displays project structure in a hierarchical tree format, similar to unix `tree` command
- **Filter organization**: Shows files organized by their Visual Studio filters/folders
- **File display**: Always shows file extensions for clarity
- **Empty filter handling**: Option to hide empty filters for cleaner output
- **Visual Studio accuracy**: Matches the exact structure as seen in Visual Studio Solution Explorer

### Example Output

For a project structure like:
```
project/
├── main.c
├── src/
│   ├── utils.c
│   └── core/
│       └── engine.c
└── MyProject.vcxproj
```

The tool will:
1. **Add**: Add ClCompile entries to MyProject.vcxproj and create/update filters
2. **View**: Display structure like:
   ```
   📁 MyProject.vcxproj
   ├── 📁 Source Files
   │   └── 📄 main.c
   └── 📁 src
       ├── 📄 utils.c
       └── 📁 core
           └── 📄 engine.c
   📊 Project summary: 3 files
      2 filters
   ```
3. **Delete**: Remove files and clean up empty filters automatically

## Supported File Extensions

The tool currently supports C and C++ source files:
- `.c`
- `.cpp`  
- `.cc`
- `.cxx`

Additional extensions can be easily added by modifying the source code.

## Dependencies

- `clap` - Command line argument parsing
- `walkdir` - Directory traversal
- `uuid` - Generate unique identifiers for filters
- `anyhow` - Error handling

## Contributing

Contributions are welcome! Feel free to open issues or submit pull requests.

## License

This project is licensed under the MIT License.