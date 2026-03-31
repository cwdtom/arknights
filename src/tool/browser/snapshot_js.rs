pub(crate) const SNAPSHOT_JS: &str = r#"() => {
  const trim = (value) => {
    const normalized = String(value || "").replace(/\s+/g, " ").trim();
    return normalized.slice(0, 160);
  };
  const visible = (element) => {
    const style = window.getComputedStyle(element);
    if (style.display === "none" || style.visibility === "hidden") return false;
    return element.getClientRects().length > 0;
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
    const fillable = ["input", "select", "textarea"].includes(tag) || element.isContentEditable;
    const clickable =
      !fillable ||
      tag === "a" ||
      tag === "button" ||
      element.getAttribute("role") === "button" ||
      element.hasAttribute("onclick");
    index += 1;
    const arkId = `e_${index}`;
    element.setAttribute("data-ark-id", arkId);
    elements.push({
      id: arkId,
      tag,
      role: trim(element.getAttribute("role")),
      html_id: trim(element.id),
      name: trim(element.getAttribute("name")),
      type: trim(element.getAttribute("type")),
      text: trim(element.innerText || element.textContent || ""),
      placeholder: trim(element.getAttribute("placeholder")),
      aria_label: trim(element.getAttribute("aria-label")),
      href: trim(element.getAttribute("href")),
      clickable,
      fillable,
      visible: true
    });
  }

  return {
    url: window.location.href,
    title: document.title,
    viewport_width: window.innerWidth || 0,
    viewport_height: window.innerHeight || 0,
    scroll_y: Math.round(window.scrollY || window.pageYOffset || 0),
    document_height: Math.max(
      document.documentElement ? document.documentElement.scrollHeight : 0,
      document.body ? document.body.scrollHeight : 0
    ),
    elements
  };
}"#;
