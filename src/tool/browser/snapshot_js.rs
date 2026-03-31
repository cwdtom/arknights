pub(crate) const SNAPSHOT_JS: &str = r#"() => {
  const trim = (value) => {
    const normalized = String(value || "").replace(/\s+/g, " ").trim();
    return normalized.slice(0, 160);
  };
  const setIfPresent = (target, key, value) => {
    if (value) target[key] = value;
  };
  const visible = (element) => {
    const style = window.getComputedStyle(element);
    if (style.display === "none" || style.visibility === "hidden") return false;
    return element.getClientRects().length > 0;
  };
  const inferKind = (element, tag) => {
    if (tag === "input") return "input";
    if (tag === "select") return "select";
    if (tag === "textarea") return "textarea";
    if (tag === "a" && element.getAttribute("href")) return "link";
    if (
      tag === "button" ||
      element.getAttribute("role") === "button" ||
      element.hasAttribute("onclick")
    ) {
      return "button";
    }
    if (element.isContentEditable) return "editable";
    return tag;
  };
  const interactive = [
    "a[href]",
    "button",
    "input",
    "select",
    "textarea",
    "[role='button']",
    "[contenteditable='true']",
    "[tabindex]:not([tabindex='-1'])"
  ];

  document.querySelectorAll("[data-ark-id]").forEach((element) => {
    element.removeAttribute("data-ark-id");
  });

  let index = 0;
  const elements = [];
  for (const element of document.querySelectorAll(interactive.join(","))) {
    if (!visible(element)) continue;
    const tag = element.tagName.toLowerCase();
    index += 1;
    const arkId = `e_${index}`;
    element.setAttribute("data-ark-id", arkId);
    const snapshot = {
      id: arkId,
      kind: inferKind(element, tag)
    };
    setIfPresent(snapshot, "html_id", trim(element.id));
    setIfPresent(snapshot, "name", trim(element.getAttribute("name")));
    setIfPresent(snapshot, "type", trim(element.getAttribute("type")));
    setIfPresent(snapshot, "text", trim(element.innerText || element.textContent || ""));
    setIfPresent(snapshot, "placeholder", trim(element.getAttribute("placeholder")));
    setIfPresent(snapshot, "aria_label", trim(element.getAttribute("aria-label")));
    setIfPresent(snapshot, "href", trim(element.getAttribute("href")));
    elements.push(snapshot);
  }

  return {
    url: window.location.href,
    title: document.title,
    scroll_y: Math.round(window.scrollY || window.pageYOffset || 0),
    document_height: Math.max(
      document.documentElement ? document.documentElement.scrollHeight : 0,
      document.body ? document.body.scrollHeight : 0
    ),
    elements
  };
}"#;
