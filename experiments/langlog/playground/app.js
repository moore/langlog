import init, { build, buildAndRunReady, check } from "./pkg/langlog_playground_wasm.js";

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
let examples = [];
let exampleSlugs = [];
let exampleIndexBySlug = new Map();
const docsCache = new Map();

async function ensureInitialized() {
  if (!initialized) {
    await init();
    initialized = true;
  }
}

async function loadExamples() {
  const response = await fetch("examples.json");
  if (!response.ok) {
    throw new Error("failed to load playground examples");
  }
  examples = await response.json();
  if (!Array.isArray(examples) || examples.length === 0) {
    throw new Error("playground examples are empty or invalid");
  }
  exampleSlugs = [];
  exampleIndexBySlug = new Map();
  examplesSelect.replaceChildren();
  examples.forEach((example, index) => {
    const slug = uniqueExampleSlug(example.name, index);
    exampleSlugs.push(slug);
    exampleIndexBySlug.set(slug, index);

    const option = document.createElement("option");
    option.value = slug;
    option.textContent = example.name;
    examplesSelect.append(option);
  });
  stdin.value = "42";
  selectExampleFromHash();
}

function uniqueExampleSlug(name, index) {
  const base = String(name || `example-${index + 1}`)
    .toLowerCase()
    .replaceAll("&", "and")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "") || `example-${index + 1}`;
  let slug = base;
  let suffix = 2;
  while (exampleIndexBySlug.has(slug)) {
    slug = `${base}-${suffix}`;
    suffix += 1;
  }
  return slug;
}

function exampleIndexFromHash() {
  const raw = window.location.hash.startsWith("#")
    ? window.location.hash.slice(1)
    : window.location.hash;
  if (raw.length === 0) {
    return 0;
  }

  let slug = raw;
  try {
    slug = decodeURIComponent(raw);
  } catch (_error) {
    return 0;
  }

  if (exampleIndexBySlug.has(slug)) {
    return exampleIndexBySlug.get(slug);
  }

  const oneBasedIndex = Number(slug);
  if (Number.isInteger(oneBasedIndex) && oneBasedIndex >= 1 && oneBasedIndex <= examples.length) {
    return oneBasedIndex - 1;
  }

  return 0;
}

function selectExampleFromHash() {
  if (examples.length === 0) {
    return;
  }
  selectExample(exampleIndexFromHash(), { updateHash: false });
}

function selectExample(index, { updateHash } = { updateHash: false }) {
  if (examples.length === 0) {
    return;
  }
  const boundedIndex = index >= 0 && index < examples.length ? index : 0;
  examplesSelect.value = exampleSlugs[boundedIndex];
  editor.value = examples[boundedIndex].source;
  diagnostics.textContent = "";
  terminal.textContent = "";
  wat.textContent = "";
  selectOutputTab("diagnostics");

  if (updateHash) {
    updateExampleHash(boundedIndex);
  }
}

function updateExampleHash(index) {
  const nextHash = `#${encodeURIComponent(exampleSlugs[index])}`;
  if (window.location.hash !== nextHash) {
    window.location.hash = nextHash;
  }
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
  const index = exampleIndexBySlug.get(examplesSelect.value) ?? 0;
  selectExample(index, { updateHash: true });
});

window.addEventListener("hashchange", selectExampleFromHash);

Promise.all([loadExamples(), ensureInitialized()]).catch((error) => {
  diagnostics.textContent = `failed to initialize playground compiler: ${error.message}`;
});
