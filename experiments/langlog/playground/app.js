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
const docsDrawer = document.querySelector("#docsDrawer");
const docsTitle = document.querySelector("#docsTitle");
const docsContent = document.querySelector("#docsContent");
const docsCloseButton = document.querySelector("#docsCloseButton");

let initialized = false;
const docsCache = new Map();

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

async function openDocs(path) {
  docsTitle.textContent = path === "REFERENCE.md" ? "Reference" : "Tutorial";
  docsContent.innerHTML = "<p>Loading...</p>";
  docsDrawer.classList.add("open");
  docsDrawer.setAttribute("aria-hidden", "false");

  try {
    if (!docsCache.has(path)) {
      const response = await fetch(path);
      if (!response.ok) {
        throw new Error(`failed to load ${path}`);
      }
      docsCache.set(path, await response.text());
    }
    docsContent.innerHTML = renderMarkdown(docsCache.get(path));
  } catch (error) {
    docsContent.textContent = error.message;
  }
}

function closeDocs() {
  docsDrawer.classList.remove("open");
  docsDrawer.setAttribute("aria-hidden", "true");
}

function renderMarkdown(markdown) {
  const blocks = [];
  const lines = markdown.split(/\r?\n/);
  let paragraph = [];
  let list = [];
  let ordered = false;
  let code = null;

  function flushParagraph() {
    if (paragraph.length > 0) {
      blocks.push(`<p>${renderInline(paragraph.join(" "))}</p>`);
      paragraph = [];
    }
  }

  function flushList() {
    if (list.length > 0) {
      const tag = ordered ? "ol" : "ul";
      blocks.push(`<${tag}>${list.map((item) => `<li>${renderInline(item)}</li>`).join("")}</${tag}>`);
      list = [];
      ordered = false;
    }
  }

  for (const line of lines) {
    if (code) {
      if (line.startsWith("```")) {
        blocks.push(`<pre><code>${escapeHtml(code.lines.join("\n"))}</code></pre>`);
        code = null;
      } else {
        code.lines.push(line);
      }
      continue;
    }

    if (line.startsWith("```")) {
      flushParagraph();
      flushList();
      code = { lines: [] };
      continue;
    }

    const heading = /^(#{1,3})\s+(.*)$/.exec(line);
    if (heading) {
      flushParagraph();
      flushList();
      const level = heading[1].length;
      blocks.push(`<h${level}>${renderInline(heading[2])}</h${level}>`);
      continue;
    }

    const unorderedItem = /^-\s+(.*)$/.exec(line);
    if (unorderedItem) {
      flushParagraph();
      if (list.length > 0 && ordered) {
        flushList();
      }
      ordered = false;
      list.push(unorderedItem[1]);
      continue;
    }

    const orderedItem = /^\d+\.\s+(.*)$/.exec(line);
    if (orderedItem) {
      flushParagraph();
      if (list.length > 0 && !ordered) {
        flushList();
      }
      ordered = true;
      list.push(orderedItem[1]);
      continue;
    }

    if (line.trim() === "") {
      flushParagraph();
      flushList();
      continue;
    }

    paragraph.push(line.trim());
  }

  flushParagraph();
  flushList();
  if (code) {
    blocks.push(`<pre><code>${escapeHtml(code.lines.join("\n"))}</code></pre>`);
  }

  return blocks.join("\n");
}

function renderInline(text) {
  const escaped = escapeHtml(text);
  return escaped
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, (_match, label, href) => {
      const safeHref = /^[a-z][a-z0-9+.-]*:/i.test(href) && !href.startsWith("http")
        ? "#"
        : href;
      return `<a href="${escapeAttribute(safeHref)}" target="_blank" rel="noreferrer">${label}</a>`;
    });
}

function escapeHtml(value) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function escapeAttribute(value) {
  return escapeHtml(value).replaceAll("'", "&#39;");
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
docsCloseButton.addEventListener("click", closeDocs);

document.querySelectorAll("[data-doc]").forEach((button) => {
  button.addEventListener("click", () => {
    openDocs(button.dataset.doc);
  });
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && docsDrawer.classList.contains("open")) {
    closeDocs();
  }
});

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
