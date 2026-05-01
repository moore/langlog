import init, { build, buildAndRunReady, check } from "./pkg/langlog_playground_wasm.js";

const examples = [
  {
    name: "Return 42",
    source: `fn main() -> u32 {
    42
}
`,
  },
  {
    name: "Print input",
    source: `fn main() -> u32 {
    let value: u32 = read_u32();
    print_u32(value);
    print_newline();
    value
}
`,
  },
  {
    name: "Branch",
    source: `fn main() -> u32 {
    let value: u32 = read_u32();
    if value > 10 {
        print_bool(true);
        print_newline();
        return value;
    }
    print_bool(false);
    print_newline();
    10
}
`,
  },
  {
    name: "Repo smoke",
    source: `fn sum(values: [u32; 4]) -> u32 {
    let mut total: u32 = 0;

    for value in values {
        total = total + value;
    }

    total
}

fn bounded(total: u32, limit: u32, one: u32) -> u32 {
    observe total + one <= limit + one else {
        return total;
    }

    if total > 100 {
        observe total + 1 < 1001 else {
            return total;
        }
    }

    return total;
}
`,
  },
  {
    name: "Repo tutorial",
    source: `fn sum(values: [u32; 4]) -> u32 {
    let mut total: u32 = 0;

    for value in values {
        total = total + value;
    }

    total
}

fn bounded(total: u32, limit: u32, one: u32) -> u32 {
    observe total + one <= limit + one else {
        return total;
    }

    if total > 100 {
        observe total + 1 < 1001 else {
            return total;
        }
    }

    total
}

fn choose(flag: bool) -> u32 {
    let mut value: u32 = 0;

    match flag {
        true => { value = 1; },
        false => { value = 2; }
    }

    value
}
`,
  },
];

const editor = document.querySelector("#sourceEditor");
const diagnostics = document.querySelector("#diagnosticsOutput");
const terminal = document.querySelector("#terminalOutput");
const wat = document.querySelector("#watOutput");
const stdin = document.querySelector("#stdinInput");
const examplesSelect = document.querySelector("#exampleSelect");
const diagnosticsTab = document.querySelector("#diagnosticsTab");
const watTab = document.querySelector("#watTab");

let initialized = false;

async function ensureInitialized() {
  if (!initialized) {
    await init();
    initialized = true;
  }
}

function loadExamples() {
  examples.forEach((example, index) => {
    const option = document.createElement("option");
    option.value = String(index);
    option.textContent = example.name;
    examplesSelect.append(option);
  });
  editor.value = examples[0].source;
  stdin.value = "42";
}

function updateDiagnostics(result) {
  diagnostics.textContent =
    result.diagnostics ||
    `ok: ${result.itemCount} item(s), obligations: ${result.obligations}, observations: ${result.observations}`;
}

function showCheckResult(result) {
  updateDiagnostics(result);
  selectOutputTab("diagnostics");
}

function showBuildResult(result) {
  updateDiagnostics(result);
  wat.textContent = result.wat || "";
}

function selectOutputTab(name) {
  const showDiagnostics = name === "diagnostics";
  diagnosticsTab.classList.toggle("active", showDiagnostics);
  watTab.classList.toggle("active", !showDiagnostics);
  diagnosticsTab.setAttribute("aria-selected", String(showDiagnostics));
  watTab.setAttribute("aria-selected", String(!showDiagnostics));
  diagnostics.classList.toggle("active", showDiagnostics);
  wat.classList.toggle("active", !showDiagnostics);
}

function tokenizeInput() {
  return stdin.value.trim().length === 0 ? [] : stdin.value.trim().split(/\s+/).reverse();
}

function hostImports(inputTokens, output) {
  return {
    langlog_host: {
      read_u32() {
        const token = inputTokens.pop();
        if (token === undefined) {
          throw new Error("stdin exhausted while reading u32");
        }
        if (!/^\d+$/.test(token)) {
          throw new Error(`invalid u32 input: ${token}`);
        }
        const value = Number(token);
        if (!Number.isSafeInteger(value) || value > 0xffffffff) {
          throw new Error(`u32 input out of range: ${token}`);
        }
        return value >>> 0;
      },
      print_u32(value) {
        output.push(String(value >>> 0));
      },
      print_bool(value) {
        output.push(value === 0 ? "false" : "true");
      },
      print_newline() {
        output.push("\n");
      },
    },
  };
}

async function runProgram() {
  await ensureInitialized();
  terminal.textContent = "";
  const result = buildAndRunReady(editor.value);
  showBuildResult(result);
  if (!result.success || !result.canRun) {
    terminal.textContent = "program did not build";
    return;
  }

  const output = [];
  try {
    const inputTokens = tokenizeInput();
    const instance = await WebAssembly.instantiate(result.wasm, hostImports(inputTokens, output));
    const returnValue = instance.instance.exports.main();
    terminal.textContent = `${output.join("")}\n[main returned ${returnValue >>> 0}]`;
  } catch (error) {
    terminal.textContent = `${output.join("")}\n[runtime error] ${error.message}`;
  }
}

document.querySelector("#checkButton").addEventListener("click", async () => {
  await ensureInitialized();
  showCheckResult(check(editor.value));
});

document.querySelector("#buildButton").addEventListener("click", async () => {
  await ensureInitialized();
  showBuildResult(build(editor.value));
});

document.querySelector("#runButton").addEventListener("click", runProgram);

diagnosticsTab.addEventListener("click", () => selectOutputTab("diagnostics"));
watTab.addEventListener("click", () => selectOutputTab("wat"));

examplesSelect.addEventListener("change", () => {
  editor.value = examples[Number(examplesSelect.value)].source;
  diagnostics.textContent = "";
  terminal.textContent = "";
  wat.textContent = "";
  selectOutputTab("diagnostics");
});

loadExamples();
ensureInitialized().catch((error) => {
  diagnostics.textContent = `failed to initialize playground compiler: ${error.message}`;
});
