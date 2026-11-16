#!/usr/bin/env python3
"""Browser regressions against `cargo run --locked --example preview` only.

The preview uses fictional, in-memory chats and a temporary web store. No real
WhatsApp client is constructed. Requires the Python playwright package and its
Chromium, or PLAYWRIGHT_CHROMIUM_EXECUTABLE for an installed browser.
"""
import argparse
import os
from pathlib import Path
from urllib.parse import parse_qs, unquote, urlsplit

from playwright.sync_api import sync_playwright

TOKEN = "b" * 32  # Public fixture credential, never a production secret.
CHAT = "/chat/100@s.whatsapp.net"


def assert_small_text_contrast(page):
    ratios = page.evaluate("""() => {
      const parse = value => value.match(/[\\d.]+/g).slice(0, 3).map(Number);
      const lum = rgb => rgb.map(v => { v /= 255; return v <= .04045 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4; })
        .reduce((s, v, i) => s + v * [.2126, .7152, .0722][i], 0);
      const ratio = (a, b) => { const x = lum(a), y = lum(b); return (Math.max(x, y) + .05) / (Math.min(x, y) + .05); };
      const color = value => {
        const values = value.match(/[\\d.]+/g).map(Number);
        return [values.slice(0, 3), values.length > 3 ? values[3] : 1];
      };
      const out = {};
      for (const el of document.querySelectorAll('body *')) {
        if (typeof el.innerText !== 'string' || !el.innerText.trim() || !el.getClientRects().length || parseFloat(getComputedStyle(el).fontSize) >= 12) continue;
        const fg = color(getComputedStyle(el).color)[0];
        let node = el, bg = [255, 255, 255];
        while (node) {
          const [layer, alpha] = color(getComputedStyle(node).backgroundColor);
          bg = layer.map((v, i) => v * alpha + bg[i] * (1 - alpha));
          if (alpha === 1) break;
          node = node.parentElement;
        }
        const key = el.tagName.toLowerCase() + (el.className && typeof el.className === 'string' ? '.' + el.className.trim().replace(/\\s+/g, '.') : '');
        out[key] = Math.min(out[key] ?? Infinity, ratio(fg, bg));
      }
      return out;
    }""")
    assert all(value >= 4.5 for value in ratios.values()), ratios


def assert_latest_message_in_scroll(page):
    bounds = page.evaluate("""() => {
      const scroll = document.querySelector('.thread-scroll').getBoundingClientRect();
      const messages = document.querySelectorAll('.thread-scroll .message');
      const last = messages[messages.length - 1].getBoundingClientRect();
      return {scroll: {top: scroll.top, bottom: scroll.bottom}, last: {top: last.top, bottom: last.bottom}};
    }""")
    assert bounds["last"]["top"] >= bounds["scroll"]["top"], bounds
    assert bounds["last"]["bottom"] <= bounds["scroll"]["bottom"], bounds


def run(base_url: str, artifacts: Path):
    parsed = urlsplit(base_url)
    if parsed.hostname not in {"localhost", "127.0.0.1", "::1"}:
        raise SystemExit("Use the local fictional preview, never a real account.")
    artifacts.mkdir(parents=True, exist_ok=True)
    with sync_playwright() as p:
        executable = os.environ.get("PLAYWRIGHT_CHROMIUM_EXECUTABLE")
        browser = p.chromium.launch(
            executable_path=executable or None, args=["--no-sandbox"]
        )
        requests = []
        errors = []
        captures = [
            ("desktop", 1440, 1000, ""),
            ("desktop-short", 1366, 768, ""),
            ("mobile", 390, 844, ""),
            ("nokia", 240, 320, "&view=compact"),
            ("small-nokia", 128, 160, "&view=compact"),
        ]
        sweep = [(str(w), w, 800, "") for w in [128, 240, 320, 768, 860, 1024]]
        for name, width, height, mode in captures + sweep:
            context = browser.new_context(
                viewport={"width": width, "height": height},
                java_script_enabled=False,
                device_scale_factor=1,
            )
            page = context.new_page()
            page.on("request", lambda request: requests.append(request.url))
            page.on("pageerror", lambda error: errors.append(str(error)))
            response = page.goto(f"{base_url}{CHAT}?token={TOKEN}{mode}")
            assert response.status == 200
            assert page.locator("script, link[rel=stylesheet], img").count() == 0
            assert page.locator("#message").is_visible()
            assert_small_text_contrast(page)
            assert page.get_by_role("button", name="Send").is_visible()
            assert page.locator(".message--own").count() > 0
            assert page.locator(".message:not(.message--own)").count() > 0
            assert page.evaluate("document.documentElement.scrollWidth <= innerWidth"), name
            if width >= 860 and not mode:
                assert page.get_by_role("navigation", name="Conversations").is_visible()
                assert page.locator("a[aria-current=page]").inner_text().startswith("AM")
                received = page.locator(".message:not(.message--own) .bubble").first.bounding_box()
                own = page.locator(".message--own .bubble").first.bounding_box()
                assert own["x"] > received["x"]
            else:
                assert page.get_by_role("link", name="Back to conversations").is_visible()
                assert not page.locator(".sidebar").is_visible()
            # Long content must wrap instead of widening the viewport.
            page.locator(".message-text").first.evaluate("el => el.textContent = 'x'.repeat(4096)")
            assert page.evaluate("document.documentElement.scrollWidth <= innerWidth"), name
            page.reload()
            if name in {row[0] for row in captures}:
                page.screenshot(path=str(artifacts / f"{name}.png"), full_page=True)
            page.get_by_role("link", name="Back to conversations").click() if width < 860 or mode else page.get_by_role("link", name="Lucas Ferreira Open conversation").click()
            assert page.locator(".chat-link").count() == 7
            assert page.evaluate("document.documentElement.scrollWidth <= innerWidth"), name + " list"
            context.close()

        # A real HTTP form POST without cookies, JS or enhanced CSS.
        context = browser.new_context(
            viewport={"width": 240, "height": 320}, java_script_enabled=False
        )
        page = context.new_page()
        page.goto(f"{base_url}{CHAT}?token={TOKEN}&view=compact")
        assert "@media screen" not in page.locator("style").inner_text()
        page.evaluate("document.querySelectorAll('style').forEach(el => el.remove())")
        context.clear_cookies()
        message = "Offline form check: olá & <b>literal text</b>"
        page.get_by_label("Message", exact=True).fill(message)
        page.get_by_role("button", name="Send", exact=True).click()
        page.wait_for_url("**/chat/**")
        assert parse_qs(urlsplit(page.url).query)["view"] == ["compact"]
        assert page.locator(".message-text").filter(has_text=message).count() == 1
        assert page.url.endswith("#messages-end"), page.url
        assert page.locator("#messages-end").is_visible()
        assert_latest_message_in_scroll(page)
        assert page.locator(".message-text b").count() == 0
        page.get_by_role("link", name="Responsive view").click()
        assert "view" not in parse_qs(urlsplit(page.url).query)
        assert parse_qs(urlsplit(page.url).query)["token"] == [TOKEN]
        page.get_by_role("link", name="Compact view").click()
        assert parse_qs(urlsplit(page.url).query)["view"] == ["compact"]
        # Browser paste of 4,096 supplementary Unicode scalars remains intact.
        emoji_message = "🙂" * 4096
        page.context.grant_permissions(["clipboard-read", "clipboard-write"])
        page.evaluate("async text => navigator.clipboard.writeText(text)", emoji_message)
        textarea = page.get_by_label("Message", exact=True)
        textarea.click()
        textarea.press("Control+V")
        assert textarea.input_value() == emoji_message
        context.close()

        # The same send/refresh flow on desktop verifies the last bubble is
        # actually inside the clipped history viewport, with application JS off.
        context = browser.new_context(
            viewport={"width": 1366, "height": 768}, java_script_enabled=False
        )
        page = context.new_page()
        page.goto(f"{base_url}{CHAT}?token={TOKEN}")
        assert page.locator(".thread-scroll").evaluate(
            "el => el.scrollHeight > el.clientHeight"
        ), "desktop history fixture must overflow the scroll panel"
        page.get_by_label("Message", exact=True).fill("Desktop post scroll check")
        page.get_by_role("button", name="Send", exact=True).click()
        page.wait_for_url("**/chat/**")
        assert page.url.endswith("#messages-end"), page.url
        assert_latest_message_in_scroll(page)
        for _ in range(2):
            with page.expect_response(
                lambda response: response.request.resource_type == "document"
                and unquote(urlsplit(response.url).path) == CHAT,
                timeout=5000,
            ) as refresh_response:
                page.get_by_role("link", name="Refresh conversation").click()
            assert refresh_response.value.status == 200
            assert page.url.endswith("#messages-end"), page.url
            assert_latest_message_in_scroll(page)
        context.close()

        # Login, keyboard focus and layout selection with JS disabled.
        context = browser.new_context(
            viewport={"width": 1440, "height": 900}, java_script_enabled=False
        )
        page = context.new_page()
        page.goto(base_url + "/")
        page.screenshot(path=str(artifacts / "login.png"), full_page=True)
        page.get_by_label("Password", exact=True).focus()
        assert page.get_by_label("Password", exact=True).evaluate(
            "el => getComputedStyle(el).outlineStyle != 'none'"
        )
        page.get_by_label("Password", exact=True).fill("preview-password-only")
        page.get_by_role("button", name="Open conversations").click()
        assert page.get_by_role("heading", name="Conversations", exact=True).is_visible()
        page.screenshot(path=str(artifacts / "conversations.png"), full_page=True)
        page.get_by_role("link", name="Ana Martins Open conversation").click()
        assert "token" not in parse_qs(urlsplit(page.url).query)
        context.close()
        browser.close()
        assert not errors, errors
        assert all(urlsplit(url).netloc == parsed.netloc for url in requests), requests
        print(f"PASS: {len(captures + sweep)} viewports, navigation, no-JS/cookieless POST, CSS-free form, mode switch and login.")
        print(f"Screenshots: {artifacts.resolve()}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:3080")
    parser.add_argument("--artifacts", type=Path, default=Path("/tmp/gateway-layout"))
    args = parser.parse_args()
    run(args.base_url.rstrip("/"), args.artifacts)
