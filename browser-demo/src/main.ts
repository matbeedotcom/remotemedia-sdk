import { PipelineRunner, type PipelineManifest } from './pipeline-runner';
import './style.css';

// Example manifests
const EXAMPLES = {
  calculator: {
    manifest: {
      version: 'v1',
      metadata: {
        name: 'calculator-demo',
        description: 'Simple calculator using Rust-native nodes',
      },
      nodes: [
        { id: 'multiply', node_type: 'MultiplyNode', params: { factor: 2 } },
        { id: 'add', node_type: 'AddNode', params: { addend: 10 } },
      ],
      connections: [{ from: 'multiply', to: 'add' }],
    },
    inputData: [5, 7, 3],
  },
  'text-processor': {
    manifest: {
      version: 'v1',
      metadata: {
        name: 'text-processor-demo',
        description: 'Text processing using Python nodes',
      },
      nodes: [
        { id: 'text1', node_type: 'TextProcessorNode', params: {} },
      ],
      connections: [],
    },
    inputData: [
      { text: 'Hello WASM', operations: ['uppercase', 'word_count'] },
      { text: 'Python in Browser', operations: ['lowercase', 'char_count'] },
    ],
  },
  mixed: {
    manifest: {
      version: 'v1',
      metadata: {
        name: 'mixed-demo',
        description: 'Mixed Rust + Python pipeline',
      },
      nodes: [
        { id: 'multiply', node_type: 'MultiplyNode', params: { factor: 3 } },
        { id: 'text', node_type: 'TextProcessorNode', params: { operations: ['uppercase', 'word_count', 'char_count'] } },
      ],
      connections: [{ from: 'multiply', to: 'text' }],
    },
    inputData: [5, 7, 10],
  },
};

class App {
  private runner: PipelineRunner;
  private wasmFile: File | null = null;

  constructor() {
    this.runner = new PipelineRunner();
    this.setupEventListeners();
    console.log('RemoteMedia WASM Demo ready');
  }

  private setupEventListeners() {
    // File input
    const fileInput = document.getElementById('wasm-file') as HTMLInputElement;
    const loadWasmBtn = document.getElementById('load-wasm-btn') as HTMLButtonElement;

    fileInput?.addEventListener('change', (e) => {
      const files = (e.target as HTMLInputElement).files;
      if (files && files[0]) {
        this.wasmFile = files[0];
        const fileName = document.getElementById('wasm-file-name');
        if (fileName) {
          fileName.textContent = files[0].name;
        }
        loadWasmBtn.disabled = false;
      }
    });

    loadWasmBtn?.addEventListener('click', () => this.loadWasm());

    // Python stdlib button (for PyO3 WASM - deprecated in browser)
    const loadPythonBtn = document.getElementById('load-python-btn');
    loadPythonBtn?.addEventListener('click', () => this.loadPython());

    // Pyodide runtime button (for Python nodes in browser)
    const loadPyodideBtn = document.getElementById('load-pyodide-btn');
    loadPyodideBtn?.addEventListener('click', () => this.loadPyodide());

    // Tab switching
    document.querySelectorAll('.tab-btn').forEach((btn) => {
      btn.addEventListener('click', (e) => {
        const target = (e.target as HTMLElement).dataset.tab;
        if (target) {
          this.switchTab(target);
        }
      });
    });

    // Example cards
    document.querySelectorAll('.example-card').forEach((card) => {
      card.addEventListener('click', (e) => {
        const example = (e.currentTarget as HTMLElement).dataset.example;
        if (example) {
          this.loadExample(example as keyof typeof EXAMPLES);
        }
      });
    });

    // Execute button
    const executeBtn = document.getElementById('execute-btn');
    executeBtn?.addEventListener('click', () => this.executePipeline());
  }

  private switchTab(tab: string) {
    document.querySelectorAll('.tab-btn').forEach((btn) => {
      btn.classList.remove('active');
    });
    document.querySelectorAll('.tab-content').forEach((content) => {
      content.classList.remove('active');
    });

    document.querySelector(`[data-tab="${tab}"]`)?.classList.add('active');
    document.getElementById(`${tab}-tab`)?.classList.add('active');
  }

  private loadExample(exampleKey: keyof typeof EXAMPLES) {
    const example = EXAMPLES[exampleKey];
    const manifestEditor = document.getElementById('manifest-editor') as HTMLTextAreaElement;
    const inputDataEditor = document.getElementById('input-data-editor') as HTMLTextAreaElement;

    if (manifestEditor) {
      manifestEditor.value = JSON.stringify(example.manifest, null, 2);
    }
    if (inputDataEditor) {
      inputDataEditor.value = JSON.stringify(example.inputData, null, 2);
    }

    // Switch to custom tab to show the loaded manifest
    this.switchTab('custom');
  }

  private async loadWasm() {
    if (!this.wasmFile) return;

    const loadWasmBtn = document.getElementById('load-wasm-btn') as HTMLButtonElement;

    try {
      loadWasmBtn.disabled = true;
      this.showLoading('wasm-status', 'Loading WASM module...');

      const arrayBuffer = await this.wasmFile.arrayBuffer();
      await this.runner.loadWasm(arrayBuffer);

      const info = this.runner.getModuleInfo();
      const sizeKB = info ? (info.size / 1024).toFixed(2) : '?';

      this.showSuccess('wasm-status', `WASM runtime loaded successfully (${sizeKB} KB)`);

      // Enable execute button
      const executeBtn = document.getElementById('execute-btn') as HTMLButtonElement;
      executeBtn.disabled = false;

    } catch (error) {
      console.error('Failed to load WASM:', error);
      this.showError('wasm-status', `Failed to load WASM: ${error}`);
      loadWasmBtn.disabled = false;
    }
  }

  private async loadPython() {
    const loadPythonBtn = document.getElementById('load-python-btn') as HTMLButtonElement;

    try {
      loadPythonBtn.disabled = true;
      this.showLoading('python-status', 'Loading Python stdlib (16MB)...');
      this.showLoading('python-status', 'Note: Python stdlib for PyO3 WASM does not work in browser. Use Pyodide instead.');

      await this.runner.loadPythonStdlib();

      this.showSuccess('python-status', 'Python stdlib loaded (for native wasmtime only).');
    } catch (error) {
      console.error('Failed to load Python stdlib:', error);
      this.showError('python-status', `Failed to load Python stdlib: ${error}`);
      loadPythonBtn.disabled = false;
    }
  }

  private async loadPyodide() {
    const loadPyodideBtn = document.getElementById('load-pyodide-btn') as HTMLButtonElement;

    try {
      loadPyodideBtn.disabled = true;
      this.showLoading('pyodide-status', 'Loading Pyodide runtime (~30-40MB, cached after first load)...');

      await this.runner.loadPyodideRuntime();

      const runtimeInfo = this.runner.getRuntimeInfo();
      const version = runtimeInfo.pyodide?.version || 'unknown';

      this.showSuccess('pyodide-status', `Pyodide ${version} loaded! Python nodes now available in browser.`);
    } catch (error) {
      console.error('Failed to load Pyodide:', error);
      this.showError('pyodide-status', `Failed to load Pyodide: ${error}`);
      loadPyodideBtn.disabled = false;
    }
  }

  private async executePipeline() {
    const manifestEditor = document.getElementById('manifest-editor') as HTMLTextAreaElement;
    const inputDataEditor = document.getElementById('input-data-editor') as HTMLTextAreaElement;
    const executeBtn = document.getElementById('execute-btn') as HTMLButtonElement;

    try {
      executeBtn.disabled = true;
      this.showLoading('execution-status', 'Executing pipeline...');

      // Parse manifest
      const manifestText = manifestEditor.value || '{}';
      const manifest: PipelineManifest = JSON.parse(manifestText);

      // Parse input data
      let inputData: any[] | undefined;
      const inputText = inputDataEditor.value.trim();
      if (inputText) {
        inputData = JSON.parse(inputText);
      }

      // Execute
      const { result, metrics } = await this.runner.execute(manifest, inputData);

      // Display results
      this.displayResults(result, metrics);

      if (result.status === 'success') {
        this.showSuccess('execution-status', 'Pipeline executed successfully!');
      } else {
        this.showError('execution-status', `Execution failed: ${result.error}`);
      }

    } catch (error) {
      console.error('Execution error:', error);
      this.showError('execution-status', `Execution failed: ${error}`);
      this.displayError(error);
    } finally {
      executeBtn.disabled = false;
    }
  }

  private displayResults(result: any, metrics: any) {
    const container = document.getElementById('results-container');
    if (!container) return;

    container.innerHTML = `
      <div class="result-json">
        <pre>${JSON.stringify(result, null, 2)}</pre>
      </div>
    `;

    // Display metrics
    const metricsContainer = document.getElementById('metrics-container');
    if (metricsContainer) {
      metricsContainer.style.display = 'block';

      const execTime = document.getElementById('metric-exec-time');
      const loadTime = document.getElementById('metric-load-time');
      const totalTime = document.getElementById('metric-total-time');

      if (execTime) execTime.textContent = `${metrics.executionTimeMs.toFixed(2)}ms`;
      if (loadTime) loadTime.textContent = `${metrics.wasmLoadTimeMs.toFixed(2)}ms`;
      if (totalTime) totalTime.textContent = `${metrics.totalTimeMs.toFixed(2)}ms`;
    }
  }

  private displayError(error: any) {
    const container = document.getElementById('results-container');
    if (!container) return;

    container.innerHTML = `
      <div class="result-json">
        <pre style="color: var(--error-color);">${String(error)}</pre>
      </div>
    `;
  }

  private showSuccess(elementId: string, message: string) {
    const el = document.getElementById(elementId);
    if (!el) return;
    el.className = 'status success';
    el.textContent = `✓ ${message}`;
  }

  private showError(elementId: string, message: string) {
    const el = document.getElementById(elementId);
    if (!el) return;
    el.className = 'status error';
    el.textContent = `✗ ${message}`;
  }

  private showLoading(elementId: string, message: string) {
    const el = document.getElementById(elementId);
    if (!el) return;
    el.className = 'status loading';
    el.textContent = `⏳ ${message}`;
  }
}

// Initialize app
new App();
