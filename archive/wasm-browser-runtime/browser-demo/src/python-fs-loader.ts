/**
 * Python Filesystem Loader
 *
 * Converts python-stdlib.json into a Directory structure for @bjorn3/browser_wasi_shim
 */

import { File, Directory } from '@bjorn3/browser_wasi_shim';

interface FileEntry {
  type: 'file';
  content: string;
  encoding: 'utf8' | 'base64';
}

interface DirectoryEntry {
  type: 'directory';
  contents: Record<string, FileEntry | DirectoryEntry>;
}

type FilesystemEntry = FileEntry | DirectoryEntry;

/**
 * Convert JSON filesystem representation to WASI Directory
 */
function buildDirectory(entries: Record<string, FilesystemEntry>): Map<string, File | Directory> {
  const contents = new Map<string, File | Directory>();

  for (const [name, entry] of Object.entries(entries)) {
    if (entry.type === 'file') {
      // Create File from content
      let data: Uint8Array;

      if (entry.encoding === 'base64') {
        // Decode base64 for binary files
        const binaryString = atob(entry.content);
        data = new Uint8Array(binaryString.length);
        for (let i = 0; i < binaryString.length; i++) {
          data[i] = binaryString.charCodeAt(i);
        }
      } else {
        // UTF-8 text files
        data = new TextEncoder().encode(entry.content);
      }

      contents.set(name, new File(data));
    } else if (entry.type === 'directory') {
      // Recursively build subdirectory
      const subContents = buildDirectory(entry.contents);
      contents.set(name, new Directory(subContents));
    }
  }

  return contents;
}

/**
 * Load Python stdlib from JSON and create WASI Directory structure
 */
export async function loadPythonStdlib(): Promise<Map<string, File | Directory>> {
  console.log('Loading Python stdlib...');
  const startTime = performance.now();

  try {
    // Fetch the JSON file
    const response = await fetch('/python-stdlib.json');
    if (!response.ok) {
      throw new Error(`Failed to fetch Python stdlib: ${response.statusText}`);
    }

    const filesystemData: Record<string, FilesystemEntry> = await response.json();

    console.log('Parsing Python stdlib filesystem...');
    const parseTime = performance.now();

    // Build the Directory structure
    const contents = buildDirectory(filesystemData);

    const totalTime = performance.now() - startTime;
    const parseOnlyTime = performance.now() - parseTime;

    console.log(`✓ Python stdlib loaded in ${totalTime.toFixed(0)}ms (parse: ${parseOnlyTime.toFixed(0)}ms)`);
    console.log(`  Files: ${countFiles(filesystemData)}`);
    console.log(`  Size: ${(response.headers.get('content-length') || '?').replace(/\B(?=(\d{3})+(?!\d))/g, ',')} bytes`);

    return contents;
  } catch (error) {
    console.error('Failed to load Python stdlib:', error);
    throw error;
  }
}

/**
 * Count total files in filesystem
 */
function countFiles(entries: Record<string, FilesystemEntry>): number {
  let count = 0;
  for (const entry of Object.values(entries)) {
    if (entry.type === 'file') {
      count++;
    } else if (entry.type === 'directory') {
      count += countFiles(entry.contents);
    }
  }
  return count;
}
