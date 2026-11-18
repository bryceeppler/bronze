use rune::Icon;

pub static SEARCH: Icon = Icon {
    name: "bronze-search",
    view: 16.0,
    body: r#"<circle cx="7" cy="7" r="4.75" stroke="currentColor" stroke-width="1.5" fill="none"/><path d="M10.5 10.5L14 14" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#,
};

pub static MORE: Icon = Icon {
    name: "bronze-more",
    view: 16.0,
    body: r#"<circle cx="3.5" cy="8" r="1.25" fill="currentColor"/><circle cx="8" cy="8" r="1.25" fill="currentColor"/><circle cx="12.5" cy="8" r="1.25" fill="currentColor"/>"#,
};

pub static CHECK: Icon = Icon {
    name: "bronze-check",
    view: 16.0,
    body: r#"<path d="M4.5 8.25L7 10.75L11.75 5.5" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#,
};

pub static ADD: Icon = Icon {
    name: "bronze-add",
    view: 16.0,
    body: r#"<circle cx="8" cy="8" r="6.75" stroke="currentColor" stroke-width="1.25" fill="none"/><path d="M8 5V11M5 8H11" stroke="currentColor" stroke-width="1.25" stroke-linecap="round"/>"#,
};

pub static COPY: Icon = Icon {
    name: "bronze-copy",
    view: 24.0,
    body: r#"<rect x="8" y="8" width="12" height="12" rx="2.5" stroke="currentColor" stroke-width="1.8" fill="none"/><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#,
};

pub static KEY: Icon = Icon {
    name: "bronze-key",
    view: 24.0,
    body: r#"<circle cx="8" cy="15" r="4" stroke="currentColor" stroke-width="1.8" fill="none"/><path d="M11 12l8-8M16 7l2.5 2.5M14 9l2 2" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#,
};
