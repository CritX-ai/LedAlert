"use strict";

const guideIndex = document.querySelector(".guide-index");
const compact = window.matchMedia("(max-width: 900px)");
function adaptIndex() {
  guideIndex.open = !compact.matches;
}
adaptIndex();
compact.addEventListener("change", adaptIndex);

const articleIndex = document.querySelector(".article-index");
if (articleIndex) {
  const readingTools = document.querySelector(".reading-tools");
  const compactReading = window.matchMedia("(max-width: 1200px)");
  function adaptReadingTools() {
    readingTools.hidden = !compactReading.matches || articleIndex.getBoundingClientRect().bottom > 0;
  }
  const contentsVisibility = new IntersectionObserver(adaptReadingTools);
  contentsVisibility.observe(articleIndex);
  compactReading.addEventListener("change", adaptReadingTools);
  adaptReadingTools();
  readingTools.querySelector("a").addEventListener("click", () => {
    articleIndex.open = true;
    articleIndex.querySelector("summary").focus({ preventScroll: true });
  });
}

for (const table of document.querySelectorAll(".prose table")) {
  const region = document.createElement("div");
  region.className = "table-scroll";
  region.tabIndex = 0;
  region.setAttribute("role", "region");
  region.setAttribute("aria-label", "Scrollable reference table");
  table.before(region);
  region.append(table);
}

// Previews start on request, including when reduced motion is preferred.
const previews = document.querySelectorAll(".prose video");
const previewVisibility = new IntersectionObserver(entries => {
  for (const entry of entries) {
    if (!entry.isIntersecting) entry.target.pause();
  }
});
for (const preview of previews) previewVisibility.observe(preview);
document.addEventListener("visibilitychange", () => {
  if (document.hidden) for (const preview of previews) preview.pause();
});

const search = document.querySelector(".manual-search");
const query = search.querySelector("input");
const status = document.querySelector("#search-status");
const results = document.querySelector("#search-results");
const output = search.querySelector(".search-output");
let indexPromise;
let revision = 0;
let debounce;

function searchText(value) {
  return value.toLocaleLowerCase().replace(/[^\p{L}\p{N}]+/gu, " ").trim();
}

function loadIndex() {
  if (!indexPromise) {
    indexPromise = fetch(search.dataset.searchIndex)
      .then(response => {
        if (!response.ok) throw new Error("Search index unavailable");
        return response.json();
      })
      .then(sections => sections.map(section => ({
        section,
        title: searchText(section.title),
        pageTitle: searchText(section.page_title),
        text: searchText(section.text),
      })))
      .catch(error => {
        indexPromise = undefined;
        throw error;
      });
  }
  return indexPromise;
}

async function findAnswers() {
  const request = ++revision;
  const phrase = searchText(query.value);
  const words = phrase.split(" ").filter(Boolean);
  results.replaceChildren();
  output.hidden = false;
  if (!words.length) {
    status.textContent = "Enter a topic to search the manual.";
    return;
  }
  status.textContent = "Searching the manual…";
  try {
    const sections = await loadIndex();
    if (request !== revision) return;
    const matches = sections.flatMap(({ section, title, pageTitle, text }) => {
      if (!words.every(word => title.includes(word) || pageTitle.includes(word) || text.includes(word))) return [];
      // Heading relevance is a separate tier: body matches cannot outweigh it.
      const headingMatch = title === phrase ? 3 : title.includes(phrase) ? 2
        : words.every(word => title.includes(word)) ? 1 : 0;
      const bodyMatch = text.includes(phrase) ? 1 : 0;
      const score = words.reduce((total, word) => total + (title.includes(word) ? 10 : pageTitle.includes(word) ? 2 : 1), 0);
      return [{ section, headingMatch, bodyMatch, score }];
    }).sort((a, b) => b.headingMatch - a.headingMatch || b.bodyMatch - a.bodyMatch || b.score - a.score);
    status.textContent = matches.length
      ? `${matches.length} ${matches.length === 1 ? "result" : "results"} found. Select a result below.`
      : "No results found. Try a shorter term, or open Troubleshoot by symptom in the guide index.";
    for (const { section, headingMatch } of matches) {
      const item = document.createElement("li");
      const link = document.createElement("a");
      link.href = section.path;
      link.textContent = section.title === section.page_title
        ? section.title : `${section.title} — ${section.page_title}`;
      item.append(link);
      if (section.text) {
        const excerpt = document.createElement("p");
        const text = section.text.toLocaleLowerCase();
        const phrasePosition = text.indexOf(phrase);
        const positions = words.map(word => text.indexOf(word)).filter(position => position >= 0);
        const position = phrasePosition >= 0 ? phrasePosition : positions.length ? Math.min(...positions) : 0;
        const start = headingMatch ? 0 : Math.max(0, position - 65);
        const end = Math.min(section.text.length, start + 230);
        excerpt.textContent = `${start ? "…" : ""}${section.text.slice(start, end)}${end < section.text.length ? "…" : ""}`;
        item.append(excerpt);
      }
      results.append(item);
    }
  } catch {
    if (request !== revision) return;
    status.textContent = "Search could not load. Press Search to retry, or use the guide index; all pages remain available.";
  }
}

query.addEventListener("input", () => {
  ++revision;
  clearTimeout(debounce);
  debounce = setTimeout(findAnswers, 180);
});
search.querySelector("form").addEventListener("submit", event => {
  event.preventDefault();
  clearTimeout(debounce);
  findAnswers();
});
function closeResults() {
  ++revision;
  clearTimeout(debounce);
  output.hidden = true;
  query.focus();
}
search.querySelector(".close-search").addEventListener("click", closeResults);
search.addEventListener("keydown", event => {
  if (event.key === "Escape") {
    event.preventDefault();
    closeResults();
  }
});
