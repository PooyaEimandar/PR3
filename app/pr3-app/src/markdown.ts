// PR3's replies are Markdown as the model wrote it, and are kept that way; the
// conversation shows them as they read. Model output is untrusted, so this never parses
// HTML: it builds elements for a small subset - paragraphs, headings, lists, quotes,
// rules, code, bold, italic and strikethrough - and sets their text, so anything else,
// raw HTML included, stays text. A link shows its words and where it goes, as text, never
// as something to tap; an image is its words.

type List = { indent: number; ordered: boolean; list: HTMLOListElement | HTMLUListElement };

function node<K extends keyof HTMLElementTagNameMap>(tag: K, text?: string, className?: string): HTMLElementTagNameMap[K] {
  const made = document.createElement(tag);
  if (text !== undefined) made.textContent = text;
  if (className) made.className = className;
  return made;
}

/** `text` as elements to show. Half-written Markdown, as it streams, shows as text. */
export function markdown(text: string): DocumentFragment {
  const out = document.createDocumentFragment();
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  // Lines of the paragraph or quote being read, and the lists open at each indent.
  let paragraph: string[] = [];
  let quote: string[] = [];
  let lists: List[] = [];

  const withBreaks = (into: HTMLElement, from: string[]) => {
    from.forEach((line, i) => {
      if (i) into.append(node("br"));
      inline(line, into);
    });
  };
  const end = () => {
    if (paragraph.length) {
      const p = node("p");
      withBreaks(p, paragraph);
      out.append(p);
      paragraph = [];
    }
    if (quote.length) {
      const q = node("blockquote");
      withBreaks(q, quote);
      out.append(q);
      quote = [];
    }
  };
  const block = (made: Node) => {
    end();
    lists = [];
    out.append(made);
  };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i] ?? "";
    const fence = /^\s*(```|~~~)/.exec(line)?.[1];
    if (fence) {
      // Until the closing fence, or the end while it streams.
      const code: string[] = [];
      for (i++; i < lines.length && !(lines[i] ?? "").trimStart().startsWith(fence); i++) code.push(lines[i] ?? "");
      const pre = node("pre");
      pre.append(node("code", code.join("\n")));
      block(pre);
      continue;
    }
    if (!line.trim()) {
      // A blank line ends a paragraph; a list goes on if its next item follows.
      end();
      continue;
    }
    if (/^\s{0,3}([-*_])(\s*\1){2,}\s*$/.test(line)) {
      block(node("hr"));
      continue;
    }
    const heading = /^\s{0,3}#{1,6}\s+(.*?)[\s#]*$/.exec(line);
    if (heading) {
      const h = node("p", undefined, "heading");
      inline(heading[1] ?? "", h);
      block(h);
      continue;
    }
    const item = /^(\s*)([-*+]|(\d{1,9})[.)])\s+(.*)$/.exec(line);
    if (item) {
      end();
      const number = item[3];
      lists = addItem(out, lists, (item[1] ?? "").length, number === undefined ? null : Number(number), item[4] ?? "");
      continue;
    }
    const quoted = /^\s{0,3}>\s?(.*)$/.exec(line);
    if (quoted) {
      if (paragraph.length) end();
      lists = [];
      quote.push(quoted[1] ?? "");
      continue;
    }
    // A line indented under a list item goes on with it.
    const last = lists[lists.length - 1]?.list.lastElementChild;
    if (last && /^\s/.test(line) && !paragraph.length) {
      last.append(node("br"));
      inline(line.trim(), last);
      continue;
    }
    if (quote.length) end();
    lists = [];
    paragraph.push(line.trim());
  }
  end();
  return out;
}

/** Adds a list item at `indent`, nesting it under the item before when it is indented
 * further; returns the lists still open. `start` is an ordered item's number. */
function addItem(out: DocumentFragment, open: List[], indent: number, start: number | null, text: string): List[] {
  const lists = open.filter((l) => l.indent <= indent);
  const ordered = start !== null;
  let top = lists[lists.length - 1];
  if (top && top.indent === indent && top.ordered !== ordered) {
    lists.pop();
    top = lists[lists.length - 1];
  }
  if (!top || top.indent < indent) {
    const list = ordered ? node("ol") : node("ul");
    if (list instanceof HTMLOListElement && start !== null && start !== 1) list.start = start;
    (top?.list.lastElementChild ?? out).append(list);
    top = { indent, ordered, list };
    lists.push(top);
  }
  const li = node("li");
  inline(text, li);
  top.list.append(li);
  return lists;
}

// Pairs of marks, longest first, and what they make.
const MARKS: [string, "strong" | "em" | "s"][] = [
  ["**", "strong"],
  ["__", "strong"],
  ["~~", "s"],
  ["*", "em"],
  ["_", "em"],
];

/** Appends `text` to `into`, with code, emphasis and links; marks without a partner stay
 * text. */
function inline(text: string, into: Node): void {
  let from = 0;
  let i = 0;
  while (i < text.length) {
    const found = span(text, i);
    if (!found) {
      i++;
      continue;
    }
    if (i > from) into.appendChild(document.createTextNode(text.slice(from, i)));
    into.appendChild(found.made);
    i = from = found.end;
  }
  if (from < text.length) into.appendChild(document.createTextNode(text.slice(from)));
}

/** The span that starts at `i`, if one does. */
function span(text: string, i: number): { made: Node; end: number } | null {
  const c = text[i];
  if (c === "`") {
    const close = text.indexOf("`", i + 1);
    return close > i + 1 ? { made: node("code", text.slice(i + 1, close)), end: close + 1 } : null;
  }
  if (c === "[" || (c === "!" && text[i + 1] === "[")) {
    const link = /^!?\[([^\]]+)\]\(([^)\s]+)\)/.exec(text.slice(i));
    if (!link) return null;
    const [whole, words = "", to = ""] = link;
    const shown = c === "!" || words === to ? words : `${words} (${to})`;
    return { made: document.createTextNode(shown), end: i + whole.length };
  }
  for (const [mark, tag] of MARKS) {
    if (!text.startsWith(mark, i)) continue;
    const start = i + mark.length;
    const one = mark.length === 1;
    // Opens before a word; inside one, as in "snake_case" or "2*3*4", only ** does.
    const inWords = one || mark === "__";
    if (start >= text.length || /\s/.test(text[start] ?? "")) continue;
    if (one && text[start] === mark) continue;
    if (inWords && /\w/.test(text[i - 1] ?? "")) continue;
    let close = text.indexOf(mark, start);
    while (close !== -1) {
      const before = text[close - 1] ?? "";
      const after = text[close + mark.length] ?? "";
      const closes =
        !/\s/.test(before) && !(one && (before === mark || after === mark)) && !(inWords && /\w/.test(after));
      if (closes) break;
      close = text.indexOf(mark, close + 1);
    }
    if (close === -1) continue;
    const made = node(tag);
    inline(text.slice(start, close), made);
    return { made, end: close + mark.length };
  }
  return null;
}

/** `text` without its Markdown, for speaking it. */
export function plain(text: string): string {
  return text
    .replace(/^\s*(```|~~~).*$/gm, "")
    .replace(/^\s*(#{1,6}|>|[-*+]|\d{1,9}[.)])\s+/gm, "")
    .replace(/!?\[([^\]]+)\]\([^)\s]+\)/g, "$1")
    .replace(/\*\*|__|~~|`/g, "")
    .replace(/(^|[^\w*])[*_](?=\S)(.*?\S)[*_](?![\w*])/g, "$1$2");
}
