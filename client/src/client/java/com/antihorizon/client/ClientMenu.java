package com.antihorizon.client;

import com.mojang.blaze3d.platform.InputConstants;
import net.fabricmc.loader.api.FabricLoader;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphics;
import net.minecraft.client.gui.components.PlayerFaceRenderer;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.input.CharacterEvent;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.client.resources.sounds.SimpleSoundInstance;
import net.minecraft.network.chat.Component;
import net.minecraft.sounds.SoundEvents;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.function.BooleanSupplier;
import java.util.function.Consumer;
import java.util.function.Function;
import java.util.Map;
import java.util.HashMap;
import java.nio.file.Files;

/**
 * The Anti-Horizon menu, opened with Right Shift. Like the launcher's home screen: a glass bar slides in at the top right
 * with a tab for each page and a lens that glides to the open one, who's playing beside it, the brand at the top left,
 * and the page itself on a glass panel underneath, on the right too. Drawn by hand each frame over the game's own background blur.
 * Every clickable thing registers its area while it is drawn, so what you see is exactly what you can click.
 */
public final class ClientMenu extends Screen {
	enum Page {
		MODS("Mods"), KEYBINDS("Keybinds");

		final String title;

		Page(String title) {
			this.title = title;
		}
	}

	/** A card on the Mods page: a HUD piece or a mod, with a switch (on, set) and/or a settings screen (config). */
	record Module(String name, String category, String glyph, String about, String modId, BooleanSupplier on, Consumer<Boolean> set,
			Function<Screen, Screen> config) {
	}

	private record Hit(int x1, int y1, int x2, int y2, Runnable action) {
		boolean has(double x, double y) {
			return x >= x1 && x < x2 && y >= y1 && y < y2;
		}
	}

	/** True while the Keybinds page waits for a new key, so that press doesn't also toggle the menu. */
	static boolean listening;
	private static Page lastPage = Page.MODS;
	private static final int BAR_TOP = 8;
	private static final int BAR_H = 24;
	/** How far the bar and the panel sit from the right edge of the screen. */
	private static final int EDGE = 12;
	private static final int SEARCH_MAX = 130;

	private final Screen parent;
	private final Ui.Motion motion = new Ui.Motion();
	private final List<Hit> hits = new ArrayList<>();
	private final List<Module> modules;
	private final String version = FabricLoader.getInstance().getModContainer("minecraft")
			.map(c -> c.getMetadata().getVersion().getFriendlyString()).orElse("");
	private Page page;
	private String category = "All";
	private String query = "";
	private boolean searching;
	private long caretAt;
	private long openedAt;
	private long pageAt;
	private long sheenAt;
	private int px;
	private int py;
	private int pw;
	private int ph;
	// The Mods grid scrolls: how far, how far it can, and where it is (for the mouse wheel).
	private float scroll;
	private int maxScroll;
	private int gridX1;
	private int gridX2;
	private int gridTop;
	private int gridBottom;

	ClientMenu(Screen parent) {
		super(Component.literal("Anti-Horizon"));
		this.parent = parent;
		this.modules = modules(Minecraft.getInstance());
	}

	Screen parent() {
		return parent;
	}

	/** Every HUD module, then the utilities that change how the game plays rather than what it shows. */
	private static List<Module> modules(Minecraft mc) {
		List<Module> list = new ArrayList<>();
		for (Hud.Module m : Hud.ALL) list.add(new Module(m.name, "HUD", m.glyph, m.about, null, m::on, m::set, null));
		if (Cosmetics.RELEASED) list.add(new Module("Cosmetics", "Mods", "AH", "Anti-Horizon capes on players, some of them live. Pick yours in the launcher.", null,
				() -> Config.cosmetics, on -> {
					Config.cosmetics = on;
					Config.save();
				}, null));
		for (LiveMods.Live l : LiveMods.find()) {
			String glyph = l.name().replaceAll("[^A-Za-z0-9]", "");
			glyph = glyph.substring(0, Math.min(3, glyph.length())).toUpperCase(Locale.ROOT);
			list.add(new Module(l.name(), "Mods", glyph, l.about(), l.modId(), l.on(), l.set(), l.config()));
		}
		return list;
	}

	// ---------------------------------------------------------------- set-up

	/** The bar's tabs: each opens a page, except HUD, which opens the HUD editor. */
	private enum Tab {
		MODS("Mods", "grid", Page.MODS),
		HUD("HUD", "hud", null),
		KEYBINDS("Keybinds", "key", Page.KEYBINDS);

		final String label;
		final String icon;
		final Page page;

		Tab(String label, String icon, Page page) {
			this.label = label;
			this.icon = icon;
			this.page = page;
		}
	}

	private void layout() {
		pw = Math.min(width - 2 * EDGE, 480);
		px = width - EDGE - pw;
		py = BAR_TOP + BAR_H + 10;
		ph = Math.max(120, Math.min(height - py - 10, 300));
	}

	@Override
	protected void init() {
		long now = System.currentTimeMillis();
		if (openedAt == 0) {
			openedAt = now;
			sheenAt = now + 120; // the light sweeps across once as the bar lands
		}
		pageAt = now;
		if (page == null) page = Config.rememberPage ? lastPage : Page.MODS;
		layout();
	}

	/** True while the search box has the keyboard, so a menu key that is a letter types instead of closing. */
	boolean typing() {
		return searching;
	}

	private void setQuery(String v) {
		query = v;
		scroll = 0;
		caretAt = System.currentTimeMillis();
		// Typing a search always shows the Mods page, where the results are.
		if (!v.isEmpty() && page != Page.MODS) go(Page.MODS);
	}

	// ---------------------------------------------------------------- drawing

	@Override
	public void renderBackground(GuiGraphics g, int mouseX, int mouseY, float delta) {
		super.renderBackground(g, mouseX, mouseY, delta);
		motion.tick();
		hits.clear();
		float open = Config.animations ? Ui.ease((System.currentTimeMillis() - openedAt) / 260f) : 1f;
		// The bar slides in from the right edge of the screen; the page follows it in underneath.
		drawBar(g, mouseX, mouseY, open);
		float slide = Ui.snap((1f - open) * 28f);
		Ui.fade = open;
		if (slide != 0) {
			g.pose().pushMatrix();
			g.pose().translate(slide, 0);
		}
		drawPanel(g, mouseX - Math.round(slide), mouseY);
		if (slide != 0) g.pose().popMatrix();
		Ui.fade = 1f;
	}

	/** The glass bar at the top right: a tab for each page with a lens under the open one, close, who's playing, the brand. */
	private void drawBar(GuiGraphics g, int mx, int my, float alpha) {
		float before = Ui.fade;
		Ui.fade = alpha;
		Tab[] tabs = Tab.values();
		float[] tw = new float[tabs.length];
		float[] tx = new float[tabs.length];
		float pill = 6;
		for (int i = 0; i < tabs.length; i++) {
			tw[i] = 10 + 8 + 5 + Txt.widthF(font, tabs[i].label, Txt.UI) + 11;
			pill += tw[i];
		}
		float y = BAR_TOP;
		// Where the bar rests, against the right edge, and how far off it still is while it slides in.
		float rest = Ui.snap(width - EDGE - BAR_H - 6 - pill);
		float bx = rest + Ui.snap((1f - alpha) * (pill + BAR_H + 6 + EDGE));
		Ui.glass(g, bx, y, bx + pill, y + BAR_H, BAR_H / 2f);

		int active = 0;
		float x = bx + 3;
		for (int i = 0; i < tabs.length; i++) {
			tx[i] = x;
			x += tw[i];
			if (tabs[i].page == page) active = i;
		}
		// The lens: a lighter piece of glass under the open page that glides from tab to tab.
		float lx = bx + motion.to("lens.x", tx[active] - bx, 18f);
		float lw = motion.to("lens.w", tw[active], 18f);
		Ui.round(g, lx, y + 3, lx + lw, y + BAR_H - 3, (BAR_H - 6) / 2f, Ui.c(Ui.BONE, 0.13f));
		Ui.hline(g, lx + 7, lx + lw - 7, y + 3, Ui.c(Ui.BONE, 0.32f));

		// A sheen sweeps across the glass when the page changes.
		long since = System.currentTimeMillis() - sheenAt;
		if (Config.animations && since >= 0 && since < 650) {
			float t = Ui.ease(since / 650f);
			float sx = bx - 20 + (pill + 40) * t;
			float step = 1f / Ui.scale();
			for (float k = -9; k <= 9; k += step) {
				float at = sx + k;
				if (at < bx + 10 || at > bx + pill - 10) continue;
				float a = (float) Math.exp(-k * k / 20.0) * 0.2f * (1f - t * 0.5f);
				Ui.vline(g, at, y + 3, y + BAR_H - 3, Ui.c(Ui.BONE, a));
			}
		}

		for (int i = 0; i < tabs.length; i++) {
			Tab tab = tabs[i];
			boolean hover = inside(mx, my, (int) tx[i], (int) y, (int) (tx[i] + tw[i]), (int) (y + BAR_H));
			int color = i == active ? Ui.c(Ui.accent(), 1f) : Ui.c(hover ? Ui.BONE : Ui.SOFT, 1f);
			Icons.draw(g, tab.icon, tx[i] + 10, y + 8, color);
			Txt.middle(g, font, tab.label, Txt.UI, tx[i] + 23, y, BAR_H, color);
			hits.add(new Hit((int) tx[i], (int) y, (int) (tx[i] + tw[i]), (int) (y + BAR_H), () -> open(tab)));
		}

		// Close: a round glass button beside the bar that turns coral under the mouse.
		float cx = bx + pill + 6;
		boolean hoverClose = inside(mx, my, (int) cx, (int) y, (int) (cx + BAR_H), (int) (y + BAR_H));
		float h = motion.to("close", hoverClose ? 1f : 0f, 16f);
		Ui.glass(g, cx, y, cx + BAR_H, y + BAR_H, BAR_H / 2f);
		if (h > 0.01f) Ui.round(g, cx + 2, y + 2, cx + BAR_H - 2, y + BAR_H - 2, BAR_H / 2f - 2, Ui.c(0xE3A08B, 0.9f * h));
		Icons.draw(g, "close", cx + 8, y + 8, h > 0.5f ? Ui.c(Ui.VOID, 1f) : Ui.c(Ui.SOFT, 1f));
		hits.add(new Hit((int) cx, (int) y, (int) (cx + BAR_H), (int) (y + BAR_H), this::onClose));

		// Who's playing sits just left of the bar and the brand at the top left, when the window is wide enough.
		String name = minecraft.getUser().getName();
		String sub = "Fabric " + version;
		float chipW = 26 + Math.max(Txt.widthF(font, name, Txt.BOLD), Txt.widthF(font, sub, Txt.SMALL)) + 12;
		float brandW = BAR_H;
		boolean brand = rest - 14 - 16 >= brandW;
		if (rest - 8 - chipW >= (brand ? 14 + brandW + 16 : 14)) {
			float chip = bx - 8 - chipW;
			Ui.glass(g, chip, y, chip + chipW, y + BAR_H, BAR_H / 2f);
			if (minecraft.player != null) PlayerFaceRenderer.draw(g, minecraft.player.getSkin(), Math.round(chip + 5), Math.round(y + 4), 16);
			Txt.base(g, font, name, Txt.BOLD, chip + 26, y + 11, Ui.c(Ui.BONE, 1f));
			Txt.base(g, font, sub, Txt.SMALL, chip + 26, y + 20, Ui.c(Ui.MUTE, 1f));
		}
		MenuVideo.Tex logo = MenuVideo.logo();
		if (brand && logo != null) MenuVideo.drawPicture(g, logo, 12, Math.round(y), BAR_H, Ui.fade);
		Ui.fade = before;
	}

	/** The open page, on a glass panel under the bar. */
	private void drawPanel(GuiGraphics g, int mx, int my) {
		Ui.glass(g, px, py, px + pw, py + ph, 10);
		int cx = px + 14;
		int cw = pw - 28;
		int top = py + 12;
		int bottom = py + ph - 10;
		float pageIn = Config.animations ? Ui.ease((System.currentTimeMillis() - pageAt) / 180f) : 1f;
		float before = Ui.fade;
		Ui.fade = before * pageIn;
		int slide = Math.round((1f - pageIn) * 6f);
		switch (page) {
			case MODS -> drawMods(g, cx, top + slide, cw, bottom, mx, my);
			case KEYBINDS -> drawKeybinds(g, cx, top + slide, cw, mx, my);
		}
		Ui.fade = before;
	}

	/** The search pill. Its text and cursor are drawn here in the smooth font, so they always line up. */
	private void drawSearch(GuiGraphics g, int sx, int y, int w, int mx, int my) {
		boolean focused = searching;
		Ui.round(g, sx, y, sx + w, y + 16, 8, Ui.c(Ui.BONE, focused ? 0.14f : 0.07f));
		Icons.draw(g, "search", sx + 7, y + 4, Ui.c(focused ? Ui.accent() : Ui.MUTE, 1f));
		float tx = sx + 20;
		float room = w - 28;
		String shown = query;
		while (!shown.isEmpty() && Txt.widthF(font, shown, Txt.UI) > room) shown = shown.substring(1);
		if (shown.isEmpty()) Txt.middle(g, font, "Search", Txt.UI, tx, y, 16, Ui.c(Ui.MUTE, focused ? 0.6f : 1f));
		else Txt.middle(g, font, shown, Txt.UI, tx, y, 16, Ui.c(Ui.BONE, 1f));
		if (focused && (System.currentTimeMillis() - caretAt) % 1000 < 500) {
			float caret = tx + (shown.isEmpty() ? 0f : Txt.widthF(font, shown, Txt.UI) + 1f);
			float baseline = y + (16 + Txt.UI.cap()) / 2f;
			Ui.vline(g, caret, baseline - Txt.UI.cap() - 1f, baseline + 1.5f, Ui.c(Ui.accent(), 1f));
		}
		hits.add(new Hit(sx, y, sx + w, y + 16, () -> {
			searching = true;
			caretAt = System.currentTimeMillis();
		}));
	}

	// ---------------------------------------------------------------- pages

	private void drawMods(GuiGraphics g, int x, int y, int w, int bottom, int mx, int my) {
		// The search sits at the right end of the first row; the category chips fill the rest, wrapping if they must.
		int sw = Math.min(SEARCH_MAX, Math.max(80, w / 3));
		drawSearch(g, x + w - sw, y, sw, mx, my);
		int limit = x + w - sw - 8;
		int cx = x;
		List<String> cats = new ArrayList<>(List.of("All"));
		for (Module m : modules) if (!cats.contains(m.category())) cats.add(m.category());
		for (String cat : cats) {
			int cw = Txt.width(font, cat, Txt.UI) + 20;
			if (cx > x && cx + cw > limit) {
				cx = x;
				y += 22;
				limit = x + w;
			}
			boolean on = category.equals(cat);
			boolean hover = inside(mx, my, cx, y, cx + cw, y + 16);
			float t = motion.to("cat." + cat, on ? 1f : 0f, 14f);
			Ui.round(g, cx, y, cx + cw, y + 16, 8, Ui.c(Ui.BONE, hover && !on ? 0.12f : 0.06f));
			if (t > 0.01f) Ui.round(g, cx, y, cx + cw, y + 16, 8, Ui.c(Ui.accent(), 0.95f * t));
			Txt.centre(g, font, cat, Txt.UI, cx + cw / 2f, y, 16, t > 0.5f ? Ui.c(Ui.VOID, 1f) : Ui.c(Ui.SOFT, 1f));
			hits.add(new Hit(cx, y, cx + cw, y + 16, () -> {
				category = cat;
				scroll = 0;
				click();
			}));
			cx += cw + 6;
		}

		String q = query.trim().toLowerCase(Locale.ROOT);
		List<Module> shown = modules.stream()
				.filter(m -> category.equals("All") || m.category().equals(category))
				.filter(m -> q.isEmpty() || m.name().toLowerCase(Locale.ROOT).contains(q))
				.toList();
		int top = y + 26;
		if (shown.isEmpty()) {
			String msg = Txt.fit(font, "Nothing matches \"" + query.trim() + "\"", Txt.UI, w);
			Txt.centre(g, font, msg, Txt.UI, x + w / 2f, top + 24, 16, Ui.c(Ui.MUTE, 1f));
			return;
		}
		int cols = Math.max(2, Math.min(6, (w + 8) / 90));
		int cardW = (w - (cols - 1) * 8) / cols;
		int cardH = 58;
		int viewBottom = bottom - 14;
		int rows = (shown.size() + cols - 1) / cols;
		int content = rows * (cardH + 8) - 8;
		maxScroll = Math.max(0, content - (viewBottom - top) + 4);
		scroll = Math.max(0, Math.min(maxScroll, scroll));
		float shift = motion.to("scroll", scroll, 18f);
		gridX1 = x;
		gridX2 = x + w;
		gridTop = top;
		gridBottom = viewBottom;
		boolean inView = my >= top && my < viewBottom;
		Module hovered = null;
		// Cards are cut off at the edges of the grid, and only their visible part can be clicked.
		g.enableScissor(x - 4, top - 4, x + w + 4, viewBottom);
		for (int i = 0; i < shown.size(); i++) {
			int x1 = x + (i % cols) * (cardW + 8);
			int y1 = top + (i / cols) * (cardH + 8) - Math.round(shift);
			if (y1 + cardH < top - 4 || y1 > viewBottom) continue;
			int before = hits.size();
			if (card(g, shown.get(i), x1, y1, cardW, cardH, inView ? mx : -1, inView ? my : -1)) hovered = shown.get(i);
			for (int k = before; k < hits.size(); k++) {
				Hit h = hits.get(k);
				hits.set(k, new Hit(h.x1(), Math.max(h.y1(), top), h.x2(), Math.min(h.y2(), viewBottom), h.action()));
			}
		}
		g.disableScissor();
		if (maxScroll > 0) {
			// A slim bar beside the grid: how much there is, and where you are in it.
			float view = viewBottom - top;
			float bar = Math.max(16, view * view / (content + 4));
			float by = top + (view - bar) * (shift / maxScroll);
			Ui.round(g, x + w + 4, by, x + w + 6, by + bar, 1, Ui.c(Ui.accent(), 0.55f));
		}
		String hint = hovered != null ? hovered.name() + ":  " + hovered.about() : "Click a card to switch it on or off. The gear opens a mod's settings.";
		Txt.base(g, font, Txt.fit(font, hint, Txt.SMALL, w), Txt.SMALL, x, bottom - 1, Ui.c(hovered != null ? Ui.SOFT : Ui.MUTE, 1f));
	}

	/** Draws one card: a switch (HUD pieces, mods that switch live) and/or a mod's settings. Returns whether hovered. */
	private boolean card(GuiGraphics g, Module m, int x, int y, int w, int h, int mx, int my) {
		boolean toggles = m.on() != null;
		boolean on = toggles && m.on().getAsBoolean();
		boolean hover = inside(mx, my, x, y, x + w, y + h);
		float lift = motion.to("card." + m.name(), hover ? 1f : 0f, 16f);
		float t = motion.to("on." + m.name(), on ? 1f : 0f, 12f);
		float yy = Ui.snap(y - lift * 2f);
		Ui.round(g, x, yy, x + w, yy + h, 6, Ui.c(Ui.BONE, 0.06f + 0.05f * lift));

		// The mod's own icon, or the glyph on a tile tinted with the accent; brighter when on.
		float ty = yy + 9;
		MenuVideo.Tex icon = m.modId() == null ? null : icon(m.modId());
		if (icon != null) {
			MenuVideo.drawPicture(g, icon, Math.round(x + w / 2f - 10), Math.round(ty), 20, Ui.fade);
		} else {
			float tw = Math.max(30, Txt.widthF(font, m.glyph(), Txt.BOLD) + 14);
			float tx = Ui.snap(x + (w - tw) / 2f);
			Ui.round(g, tx, ty, tx + tw, ty + 20, 5, Ui.c(Ui.accent(), 0.12f + 0.16f * t));
			Txt.centre(g, font, m.glyph(), Txt.BOLD, x + w / 2f, ty, 20, Ui.c(Ui.accent(), 1f));
		}

		// Top corners: a status dot when it switches, a gear when it has settings.
		if (toggles) Ui.round(g, x + w - 11, yy + 6, x + w - 6, yy + 11, 2.5f, t > 0.5f ? Ui.c(Ui.accent(), 1f) : Ui.c(Ui.MUTE, 0.55f));
		if (m.config() != null) {
			int gx = toggles ? x + 5 : x + w - 13;
			int gy = Math.round(yy) + 5;
			boolean gearHover = inside(mx, my, gx - 3, gy - 3, gx + 11, gy + 11);
			Icons.draw(g, "gear", gx, gy, Ui.c(gearHover || (!toggles && hover) ? Ui.accent() : Ui.MUTE, 1f));
			if (toggles) hits.add(new Hit(gx - 3, y + 2, gx + 11, y + 16, () -> openConfig(m)));
		}

		// The label strip fills with the accent when it's on.
		float sy = yy + h - 17;
		if (t > 0.01f) Ui.roundBottom(g, x, sy, x + w, yy + h, 6, Ui.c(Ui.accent(), 0.95f * t));
		String label = Txt.fit(font, m.name(), Txt.BOLD, w - 10);
		Txt.centre(g, font, label, Txt.BOLD, x + w / 2f, sy, 17, t > 0.5f ? Ui.c(Ui.VOID, 1f) : Ui.c(Ui.BONE, 1f));

		hits.add(new Hit(x, y, x + w, y + h, () -> {
			if (!toggles) {
				openConfig(m);
				return;
			}
			m.set().accept(!on);
			Config.save();
			click();
		}));
		return hover;
	}

	private final Map<String, MenuVideo.Tex> icons = new HashMap<>();

	/** A mod's own icon from its jar, read once. */
	private MenuVideo.Tex icon(String id) {
		if (icons.containsKey(id)) return icons.get(id);
		MenuVideo.Tex tex = null;
		try {
			var c = FabricLoader.getInstance().getModContainer(id);
			var path = c.flatMap(x -> x.getMetadata().getIconPath(64));
			var file = path.flatMap(p -> c.get().findPath(p));
			if (file.isPresent()) tex = MenuVideo.picture("mod_" + id, Files.readAllBytes(file.get()));
		} catch (Exception ignored) {
			// no icon: the card shows its letters
		}
		icons.put(id, tex);
		return tex;
	}

	private void openConfig(Module m) {
		click();
		try {
			Screen s = m.config().apply(this);
			if (s != null) minecraft.setScreen(s);
		} catch (Throwable e) {
			AntiHorizonClient.LOG.warn("Couldn't open the settings of " + m.name(), e);
		}
	}

	private void drawKeybinds(GuiGraphics g, int x, int y, int w, int mx, int my) {
		int rowH = 32;
		Ui.round(g, x, y, x + w, y + rowH, 6, Ui.c(Ui.BONE, 0.05f));
		Txt.base(g, font, "Anti-Horizon menu", Txt.BOLD, x + 10, y + 13.5f, Ui.c(Ui.BONE, 1f));
		Txt.base(g, font, "Opens this menu in any world or server.", Txt.SMALL, x + 10, y + 24.5f, Ui.c(Ui.MUTE, 1f));

		String keyName = listening ? "Press a key..." : InputConstants.Type.KEYSYM.getOrCreate(Config.menuKey).getDisplayName().getString();
		int kw = Txt.width(font, keyName, Txt.UI) + 28;
		int kx = x + w - 10 - kw;
		int ky = y + 8;
		boolean hover = inside(mx, my, kx, ky, kx + kw, ky + 16);
		Ui.round(g, kx, ky, kx + kw, ky + 16, 5, listening ? Ui.c(Ui.accent(), 0.95f) : Ui.c(Ui.BONE, hover ? 0.16f : 0.09f));
		int textColor = listening ? Ui.c(Ui.VOID, 1f) : Ui.c(Ui.BONE, 1f);
		Icons.draw(g, "key", kx + 7, ky + 4, textColor);
		Txt.middle(g, font, keyName, Txt.UI, kx + 20, ky, 16, textColor);
		hits.add(new Hit(kx, ky, kx + kw, ky + 16, () -> {
			listening = !listening;
			click();
		}));

		String help = listening ? "Press the key you want. Esc cancels." : "Click the key to change it.";
		Txt.base(g, font, help, Txt.SMALL, x + 2, y + rowH + 14, Ui.c(Ui.MUTE, 1f));
	}

	private void drawPreferences(GuiGraphics g, int x, int y, int w, int mx, int my) {
		y = pref(g, x, y, w, mx, my, "Pause game", "Pause singleplayer worlds while this menu is open.",
				() -> Config.pauseGame, v -> Config.pauseGame = v);
		y = pref(g, x, y, w, mx, my, "Keybind closes menu", "Off: the key switches between this menu and the HUD editor.",
				() -> Config.keybindCloses, v -> Config.keybindCloses = v);
		y = pref(g, x, y, w, mx, my, "Remember last page", "Reopen the page you were on last time.",
				() -> Config.rememberPage, v -> Config.rememberPage = v);
		y = pref(g, x, y, w, mx, my, "Animations", "Every motion in this menu and the HUD. Off makes it all instant.",
				() -> Config.animations, v -> Config.animations = v);
		pref(g, x, y, w, mx, my, "Anti-Horizon title screen", "The aurora title screen. Off brings back Minecraft's own.",
				() -> Config.customMenu, v -> Config.customMenu = v);
	}

	private int pref(GuiGraphics g, int x, int y, int w, int mx, int my, String title, String about, BooleanSupplier get, Consumer<Boolean> set) {
		int h = 28;
		boolean on = get.getAsBoolean();
		boolean hover = inside(mx, my, x, y, x + w, y + h);
		Ui.round(g, x, y, x + w, y + h, 6, Ui.c(Ui.BONE, hover ? 0.08f : 0.05f));
		Txt.base(g, font, title, Txt.BOLD, x + 10, y + 12f, Ui.c(Ui.BONE, 1f));
		Txt.base(g, font, Txt.fit(font, about, Txt.SMALL, w - 76), Txt.SMALL, x + 10, y + 22.5f, Ui.c(Ui.MUTE, 1f));
		int sx = x + w - 34;
		int sy = y + 8;
		String state = on ? "On" : "Off";
		Txt.middle(g, font, state, Txt.SMALL, sx - 6 - Txt.widthF(font, state, Txt.SMALL), sy, 12, Ui.c(Ui.MUTE, 1f));
		toggle(g, title, on, sx, sy);
		hits.add(new Hit(x, y, x + w, y + h, () -> {
			set.accept(!on);
			Config.save();
			click();
		}));
		return y + h + 5;
	}

	/** A switch: the track fills with the accent and the knob slides across. */
	private void toggle(GuiGraphics g, String key, boolean on, int x, int y) {
		float t = motion.to("switch." + key, on ? 1f : 0f, 16f);
		Ui.round(g, x, y, x + 22, y + 12, 6, Ui.c(Ui.BONE, 0.16f * (1f - t)));
		if (t > 0.01f) Ui.round(g, x, y, x + 22, y + 12, 6, Ui.c(Ui.accent(), t));
		float kx = Ui.snap(x + 2 + 10f * t);
		Ui.round(g, kx, y + 2, kx + 8, y + 10, 4, Ui.c(t > 0.5f ? Ui.VOID : Ui.BONE, 1f));
	}

	// ---------------------------------------------------------------- small helpers

	private static boolean inside(int mx, int my, int x1, int y1, int x2, int y2) {
		return mx >= x1 && mx < x2 && my >= y1 && my < y2;
	}

	private void click() {
		minecraft.getSoundManager().play(SimpleSoundInstance.forUI(SoundEvents.UI_BUTTON_CLICK, 1f));
	}

	// ---------------------------------------------------------------- navigation and input

	private void go(Page p) {
		listening = false;
		if (p == page) return;
		page = p;
		lastPage = p;
		pageAt = System.currentTimeMillis();
		sheenAt = pageAt;
		click();
	}

	private void open(Tab tab) {
		if (tab.page != null) {
			go(tab.page);
			return;
		}
		click();
		minecraft.setScreen(new HudEditor(this));
	}

	@Override
	public boolean mouseClicked(MouseButtonEvent e, boolean doubleClick) {
		if (e.button() == 0) {
			searching = false; // a click anywhere else lets go of the search box
			for (Hit h : List.copyOf(hits)) {
				if (h.has(e.x(), e.y())) {
					clearFocus();
					h.action().run();
					return true;
				}
			}
		}
		return super.mouseClicked(e, doubleClick);
	}

	@Override
	public boolean mouseScrolled(double x, double y, double scrollX, double scrollY) {
		if (page == Page.MODS && maxScroll > 0 && x >= gridX1 - 4 && x < gridX2 + 10 && y >= gridTop - 4 && y < gridBottom) {
			scroll = (float) Math.max(0, Math.min(maxScroll, scroll - scrollY * 33));
			return true;
		}
		return super.mouseScrolled(x, y, scrollX, scrollY);
	}

	@Override
	public boolean keyPressed(KeyEvent e) {
		if (listening) {
			if (e.key() != InputConstants.KEY_ESCAPE) {
				Config.menuKey = e.key();
				Config.save();
			}
			listening = false;
			return true;
		}
		if (searching) {
			int k = e.key();
			if (k == InputConstants.KEY_BACKSPACE) {
				if (!query.isEmpty()) setQuery(e.hasControlDown() ? "" : query.substring(0, query.offsetByCodePoints(query.length(), -1)));
			} else if (k == InputConstants.KEY_ESCAPE || k == InputConstants.KEY_RETURN || k == InputConstants.KEY_NUMPADENTER) {
				searching = false;
			} else if (k == InputConstants.KEY_V && e.hasControlDown()) {
				String pasted = minecraft.keyboardHandler.getClipboard().replaceAll("\\p{Cntrl}", "");
				setQuery(clip(query + pasted));
			}
			caretAt = System.currentTimeMillis();
			return true;
		}
		int k = e.key();
		if (k == InputConstants.KEY_LEFT || k == InputConstants.KEY_RIGHT) {
			Page[] pages = Page.values();
			int i = Math.floorMod(page.ordinal() + (k == InputConstants.KEY_RIGHT ? 1 : -1), pages.length);
			go(pages[i]);
			return true;
		}
		if (k >= InputConstants.KEY_1 && k < InputConstants.KEY_1 + Tab.values().length && k != Config.menuKey) {
			open(Tab.values()[k - InputConstants.KEY_1]);
			return true;
		}
		return super.keyPressed(e);
	}

	@Override
	public boolean charTyped(CharacterEvent e) {
		if (!searching) return super.charTyped(e);
		if (e.isAllowedChatCharacter()) setQuery(clip(query + e.codepointAsString()));
		return true;
	}

	private static String clip(String s) {
		return s.length() > 40 ? s.substring(0, 40) : s;
	}

	@Override
	public boolean isPauseScreen() {
		return Config.pauseGame;
	}

	@Override
	public void onClose() {
		listening = false;
		Config.save();
		minecraft.setScreen(parent);
	}
}
