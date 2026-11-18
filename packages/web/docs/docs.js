(function () {
	"use strict";

	function currentFile() {
		var path = window.location.pathname;
		var file = path.substring(path.lastIndexOf("/") + 1);
		return file === "" ? "index.html" : file;
	}

	function markCurrent() {
		var file = currentFile();
		var links = document.querySelectorAll(".sidebar a");
		for (var i = 0; i < links.length; i++) {
			if (links[i].getAttribute("href") === file) {
				links[i].setAttribute("aria-current", "page");
			}
		}
		var docs = document.querySelector('.top-links a[href="index.html"]');
		if (docs) {
			docs.setAttribute("aria-current", "page");
		}
	}

	function slug(text) {
		return text.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
	}

	function buildToc() {
		var toc = document.querySelector(".toc");
		if (!toc) {
			return;
		}
		var headings = document.querySelectorAll("article h2");
		if (headings.length === 0) {
			toc.hidden = true;
			return;
		}
		var list = document.createElement("ul");
		for (var i = 0; i < headings.length; i++) {
			var h = headings[i];
			if (!h.id) {
				h.id = slug(h.textContent);
			}
			var item = document.createElement("li");
			var link = document.createElement("a");
			link.href = "#" + h.id;
			link.textContent = h.textContent;
			item.appendChild(link);
			list.appendChild(item);
		}
		toc.appendChild(list);
		spy(toc, list, headings);
	}

	// Marks the section being read: the heading last scrolled past the top, or the last one once the page ends.
	function spy(toc, list, headings) {
		var links = list.querySelectorAll("a");
		var fill = document.createElement("span");
		var mark = document.createElement("span");
		fill.className = "toc-fill";
		mark.className = "toc-mark";
		list.appendChild(fill);
		list.appendChild(mark);
		var queued = false;
		function update() {
			queued = false;
			var line = parseFloat(getComputedStyle(document.documentElement).scrollPaddingTop) + 24;
			var at = 0;
			for (var i = 0; i < headings.length; i++) {
				if (headings[i].getBoundingClientRect().top <= line) at = i;
			}
			if (window.innerHeight + window.scrollY >= document.documentElement.scrollHeight - 4) at = headings.length - 1;
			for (var j = 0; j < links.length; j++) {
				links[j].classList.toggle("read", j < at);
				if (j === at) links[j].setAttribute("aria-current", "location");
				else links[j].removeAttribute("aria-current");
			}
			var a = links[at];
			list.style.setProperty("--y", a.offsetTop + "px");
			list.style.setProperty("--h", a.offsetHeight + "px");
			list.style.setProperty("--fill", a.offsetTop + "px");
			toc.classList.add("spying");
		}
		function queue() {
			if (!queued) {
				queued = true;
				requestAnimationFrame(update);
			}
		}
		window.addEventListener("scroll", queue, { passive: true });
		window.addEventListener("resize", queue);
		update();
	}

	function setupMenu() {
		var button = document.querySelector(".menu-button");
		var nav = document.getElementById("docs-nav");
		if (!button || !nav) {
			return;
		}
		function set(open) {
			document.body.classList.toggle("nav-open", open);
			button.setAttribute("aria-expanded", open ? "true" : "false");
			button.setAttribute("aria-label", open ? "Close navigation" : "Open navigation");
			document.querySelector("main").inert = open;
			var toc = document.querySelector(".toc");
			if (toc) toc.inert = open;
			if (open) nav.querySelector('[aria-current="page"]').focus();
		}
		button.addEventListener("click", function () {
			set(!document.body.classList.contains("nav-open"));
		});
		nav.addEventListener("click", function (e) {
			if (e.target.closest("a")) {
				set(false);
			}
		});
		document.addEventListener("keydown", function (e) {
			if (!document.body.classList.contains("nav-open")) return;
			if (e.key === "Tab") {
				var links = document.querySelectorAll(".topbar a, .topbar button, .sidebar a");
				var first = links[0];
				var last = links[links.length - 1];
				if (e.shiftKey && document.activeElement === first) {
					e.preventDefault();
					last.focus();
				} else if (!e.shiftKey && document.activeElement === last) {
					e.preventDefault();
					first.focus();
				}
			}
			if (e.key === "Escape") {
				set(false);
				button.focus();
			}
		});
		window.matchMedia("(min-width: 861px)").addEventListener("change", function (e) {
			if (e.matches) set(false);
		});
	}

	// navigator.clipboard is missing or refuses on file:// in some browsers, so fall back to a hidden textarea.
	function copyText(text) {
		if (navigator.clipboard && window.isSecureContext) {
			return navigator.clipboard.writeText(text).catch(function () {
				return copyWithTextarea(text);
			});
		}
		return copyWithTextarea(text);
	}

	function copyWithTextarea(text) {
		return new Promise(function (resolve, reject) {
			var area = document.createElement("textarea");
			area.value = text;
			area.setAttribute("readonly", "");
			area.style.position = "fixed";
			area.style.top = "-1000px";
			document.body.appendChild(area);
			area.select();
			var ok = false;
			try {
				ok = document.execCommand("copy");
			} catch (err) {
				ok = false;
			}
			document.body.removeChild(area);
			if (ok) {
				resolve();
			} else {
				reject(new Error("copy failed"));
			}
		});
	}

	function addCopyButtons() {
		var blocks = document.querySelectorAll("article pre");
		for (var i = 0; i < blocks.length; i++) {
			var pre = blocks[i];
			var wrap = document.createElement("div");
			wrap.className = "code";
			pre.parentNode.insertBefore(wrap, pre);
			wrap.appendChild(pre);
			var button = document.createElement("button");
			button.type = "button";
			button.className = "copy";
			button.textContent = "Copy";
			button.setAttribute("aria-label", "Copy code");
			button.addEventListener("click", onCopy(pre, button));
			wrap.appendChild(button);
		}
	}

	function onCopy(pre, button) {
		return function () {
			copyText(pre.innerText.replace(/\n$/, "")).then(
				function () {
					flash(button, "Copied");
				},
				function () {
					flash(button, "Copy failed");
				}
			);
		};
	}

	function flash(button, label) {
		button.textContent = label;
		setTimeout(function () {
			button.textContent = "Copy";
		}, 1600);
	}

	document.addEventListener("DOMContentLoaded", function () {
		markCurrent();
		buildToc();
		setupMenu();
		addCopyButtons();
	});
})();
