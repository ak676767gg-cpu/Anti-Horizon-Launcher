package com.antihorizon.client;

import net.minecraft.client.Minecraft;
import net.minecraft.client.Options;
import net.minecraft.client.gui.Gui;
import net.minecraft.client.gui.GuiGraphics;
import net.minecraft.client.multiplayer.PlayerInfo;
import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.core.BlockPos;
import net.minecraft.network.chat.Component;
import net.minecraft.world.effect.MobEffectInstance;
import net.minecraft.world.effect.MobEffects;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.LightLayer;
import net.minecraft.world.phys.EntityHitResult;

import java.time.LocalTime;
import java.time.format.DateTimeFormatter;
import java.time.format.FormatStyle;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.Deque;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.function.BiFunction;

/**
 * The HUD: small pieces on solid navy with a gold edge, each one a module the player switches on in the menu and
 * places and sizes in the HUD editor. Sizes are in GUI units at 100%; every piece is drawn at its own scale.
 */
final class Hud {
	private Hud() {
	}

	// ---------------------------------------------------------------- colours and styles

	private static final int MINT = 0xB5E6CE;
	private static final int GOLD = 0xD8BC8D;
	private static final int CORAL = 0xE3A08B;
	/** Part colours that depend on the style or theme rather than being fixed. */
	static final int UNIT = -1;
	static final int ACCENT = -2;

	static int rgb(int c) {
		if (c == UNIT) return Ui.SOFT;
		if (c == ACCENT) return Ui.accent();
		return c;
	}

	/** What every piece sits on: solid navy, nothing of the world showing through, with a faint light top edge. */
	static void panel(GuiGraphics g, float w, float h) {
		Ui.round(g, 0, 0, w, h, 3, Ui.c(Ui.VOID, 1f));
		Ui.hline(g, 3, w - 3, 0, Ui.c(Ui.BONE, 0.14f));
	}

	/** The gold mark at a chip's left edge. */
	static void edge(GuiGraphics g, float h) {
		Ui.round(g, 3, h / 2f - 3.5f, 4.5f, h / 2f + 3.5f, 0.75f, Ui.c(Ui.accent(), 1f));
	}

	/** Text in a row of a piece. */
	static void text(GuiGraphics g, Minecraft mc, String s, Txt.Face f, float x, float top, float h, int color) {
		Txt.middle(g, mc.font, s, f, x, top, h, color);
	}

	// ---------------------------------------------------------------- chips made of parts

	/** One run of a chip: some text in a face and colour, or a small icon; gap is the space before it. */
	record Part(String text, Txt.Face face, int color, float gap, String icon) {
		static Part value(String t, int color) {
			return new Part(t, Txt.BOLD, color, 0, null);
		}

		static Part unit(String t) {
			return new Part(t, Txt.SMALL, UNIT, 3, null);
		}

		static Part label(String t, float gap) {
			return new Part(t, Txt.SMALL, ACCENT, gap, null);
		}

		static Part icon(String name, int color) {
			return new Part(null, null, color, 0, name);
		}

		Part after(float g) {
			return new Part(text, face, color, g, icon);
		}
	}

	/** A value and its unit, like "120" "FPS". */
	static Part[] vu(String value, String unit) {
		return vu(value, unit, Ui.BONE);
	}

	static Part[] vu(String value, String unit, int color) {
		return unit.isEmpty() ? new Part[] {Part.value(value, color)} : new Part[] {Part.value(value, color), Part.unit(unit)};
	}

	static float partsWidth(Minecraft mc, Part[] parts) {
		float w = 0;
		for (Part p : parts) w += p.gap() + (p.icon() != null ? 8 : Txt.widthF(mc.font, p.text(), p.face()));
		return w;
	}

	static int chipWidth(Minecraft mc, Part[] parts) {
		return (int) Math.ceil(partsWidth(mc, parts)) + 14;
	}

	static void chip(GuiGraphics g, Minecraft mc, Part[] parts) {
		int w = chipWidth(mc, parts);
		panel(g, w, 14);
		edge(g, 14);
		float x = 8;
		for (Part p : parts) {
			x += p.gap();
			if (p.icon() != null) {
				Icons.draw(g, p.icon(), x, 3, Ui.c(rgb(p.color()), 1f));
				x += 8;
			} else {
				text(g, mc, p.text(), p.face(), x, 0, 14, Ui.c(rgb(p.color()), 1f));
				x += Txt.widthF(mc.font, p.text(), p.face());
			}
		}
	}

	// ---------------------------------------------------------------- modules

	/** One piece of the HUD. */
	abstract static class Module {
		final String id;
		final String name;
		final String category;
		final String glyph;
		final String about;
		final float x;
		final float y;

		Module(String id, String name, String category, String glyph, String about, float x, float y) {
			this.id = id;
			this.name = name;
			this.category = category;
			this.glyph = glyph;
			this.about = about;
			this.x = x;
			this.y = y;
		}

		abstract int width(Minecraft mc, boolean sample);

		abstract int height(Minecraft mc, boolean sample);

		/** Draws at (0, 0); the caller has already moved and scaled to where the piece sits. */
		abstract void draw(GuiGraphics g, Minecraft mc, boolean sample);

		/** Whether there is anything to show right now (no potion effects, no armour: nothing). */
		boolean shows(Minecraft mc) {
			return true;
		}

		boolean on() {
			return Config.enabled.contains(id);
		}

		void set(boolean on) {
			if (on) Config.enabled.add(id);
			else Config.enabled.remove(id);
		}

		float[] position() {
			return Config.position(id, x, y);
		}
	}

	/** A module that is one chip of text, like "120 FPS". Its text function returns null when there's nothing to show. */
	static final class Line extends Module {
		private final BiFunction<Minecraft, Boolean, Part[]> parts;

		Line(String id, String name, String category, String glyph, String about, float x, float y, BiFunction<Minecraft, Boolean, Part[]> parts) {
			super(id, name, category, glyph, about, x, y);
			this.parts = parts;
		}

		// What it showed this frame: shows(), width() and draw() each ask, and the answer can't change in between.
		private Part[] last;
		private int lastWidth;
		private int lastFrame = -1;

		private Part[] parts(Minecraft mc, boolean sample) {
			if (!sample && lastFrame == frame) return last;
			Part[] t = mc.player == null && !sample ? null : parts.apply(mc, sample);
			if (t == null && sample) t = vu("—", "");
			if (!sample) {
				last = t;
				lastWidth = t == null ? 0 : chipWidth(mc, t);
				lastFrame = frame;
			}
			return t;
		}

		@Override
		boolean shows(Minecraft mc) {
			return parts(mc, false) != null;
		}

		@Override
		int width(Minecraft mc, boolean sample) {
			Part[] t = parts(mc, sample);
			if (!sample) return lastWidth;
			return t == null ? 0 : chipWidth(mc, t);
		}

		@Override
		int height(Minecraft mc, boolean sample) {
			return 14;
		}

		@Override
		void draw(GuiGraphics g, Minecraft mc, boolean sample) {
			Part[] t = parts(mc, sample);
			if (t != null) chip(g, mc, t);
		}
	}

	static final List<Module> ALL = List.of(
			new Line("fps", "FPS Counter", "Info", "FPS", "Your frames per second; gold under 60, coral under 30.", 0f, 0f, Hud::fps),
			new Line("coords", "Coordinates", "World", "XYZ", "Your position: X, Y and Z.", 0f, 0.08f, Hud::coords),
			new Line("direction", "Direction", "World", "N", "Which way you're facing, and the axis you're walking along.", 0f, 0.16f, Hud::direction),
			new Line("ping", "Ping", "Info", "MS", "Your delay to the server; mint is good, coral is laggy.", 1f, 0.08f, Hud::ping),
			new Line("server", "Server", "Info", "IP", "The server you're on, for screenshots and videos.", 1f, 0f, Hud::server),
			new Line("cps", "CPS Counter", "PvP", "CPS", "Clicks per second, left and right.", 0f, 0.52f, Hud::cpsLine),
			new Line("combo", "Combo Counter", "PvP", "x3", "Hits in a row without being hit back; it resets when you're hit or stop for 2 seconds.", 0.5f, 0.62f, Hud::combo),
			new Line("reach", "Reach Display", "PvP", "3.0", "How far away your last hit landed, in blocks.", 0.5f, 0.68f, Hud::reach),
			new Line("saturation", "Saturation", "Player", "SAT", "Your hunger and hidden saturation, the food buffer that keeps you healing. No AppleSkin needed.", 1f, 0.92f, Hud::saturation),
			new Armour(),
			new Counter(),
			new Keys());

	static Module get(String id) {
		for (Module m : ALL) if (m.id.equals(id)) return m;
		return null;
	}

	// ---------------------------------------------------------------- drawing the HUD

	/** Smooth motion for presses and fades, shared by every piece. */
	static final Ui.Motion MOTION = new Ui.Motion();
	/** When each piece last appeared, so it fades in rather than popping up. */
	private static final Map<String, Long> APPEARED = new HashMap<>();

	/** Counts frames, so a piece works out what it shows once a frame however often it's asked. */
	static int frame;

	static void render(GuiGraphics g) {
		frame++;
		Minecraft mc = Minecraft.getInstance();
		pollClicks(mc);
		MOTION.tick();
		if (mc.options.hideGui || mc.player == null) return;
		if (mc.screen instanceof HudEditor) return; // the editor draws them itself
		if (mc.getDebugOverlay().showDebugScreen()) return; // F3 fills the screen; the HUD would only cover it
		float before = Ui.fade;
		long now = System.currentTimeMillis();
		for (Module m : ALL) {
			if (!m.on() || !m.shows(mc)) {
				APPEARED.remove(m.id);
				continue;
			}
			long since = now - APPEARED.computeIfAbsent(m.id, k -> now);
			Ui.fade = Config.animations ? Ui.ease(since / 220f) : 1f;
			int[] r = rect(m, mc, g.guiWidth(), g.guiHeight(), false);
			drawAt(m, g, mc, r, false);
		}
		Ui.fade = before;
	}

	/**
	 * A module's size, kept to steps that land on whole screen pixels (halves at GUI scale 2, thirds at 3, quarters
	 * at 4), so its smooth text and edges stay as sharp as at 100%.
	 */
	static float scaleOf(Module m) {
		int gs = Math.max(1, Minecraft.getInstance().getWindow().getGuiScale());
		return Math.max(1f / gs, Math.round(Config.scale(m.id) * gs) / (float) gs);
	}

	/** Draws a module at its place and scale. */
	static void drawAt(Module m, GuiGraphics g, Minecraft mc, int[] r, boolean sample) {
		float s = scaleOf(m);
		g.pose().pushMatrix();
		g.pose().translate(r[0], r[1]);
		g.pose().scale(s, s);
		Ui.zoom = s;
		try {
			m.draw(g, mc, sample);
		} finally {
			Ui.zoom = 1f;
			g.pose().popMatrix();
		}
	}

	/** x, y, width and height (scaled) of a module on a screen of the given size. */
	static int[] rect(Module m, Minecraft mc, int screenW, int screenH, boolean sample) {
		float s = scaleOf(m);
		int w = Math.round(m.width(mc, sample) * s);
		int h = Math.round(m.height(mc, sample) * s);
		float[] pos = m.position();
		int x = 4 + Math.round(pos[0] * Math.max(0, screenW - 8 - w));
		int y = 4 + Math.round(pos[1] * Math.max(0, screenH - 8 - h));
		return new int[] {x, y, w, h};
	}

	// ---------------------------------------------------------------- the lines

	private static Part[] fps(Minecraft mc, boolean sample) {
		int fps = sample && mc.getFps() == 0 ? 144 : mc.getFps();
		return vu(String.valueOf(fps), "FPS", fps < 30 ? CORAL : fps < 60 ? GOLD : Ui.BONE);
	}

	/** X, Y and Z each with its letter in the accent colour. */
	private static Part[] coords(Minecraft mc, boolean sample) {
		int x = 128;
		int y = 64;
		int z = -320;
		if (mc.player != null) {
			BlockPos p = mc.player.blockPosition();
			x = p.getX();
			y = p.getY();
			z = p.getZ();
		}
		return new Part[] {
				Part.label("X", 0), Part.value(String.valueOf(x), Ui.BONE).after(3),
				Part.label("Y", 7), Part.value(String.valueOf(y), Ui.BONE).after(3),
				Part.label("Z", 7), Part.value(String.valueOf(z), Ui.BONE).after(3)};
	}

	private static final String[] FACING = {"S", "SW", "W", "NW", "N", "NE", "E", "SE"};
	private static final String[] AXIS = {"+Z", "+Z", "-X", "-X", "-Z", "-Z", "+X", "+X"};

	private static Part[] direction(Minecraft mc, boolean sample) {
		float yaw = mc.player == null ? 180f : mc.player.getYRot();
		float deg = ((yaw % 360f) + 360f) % 360f;
		int i = Math.round(deg / 45f) % 8;
		int a = Math.round(deg / 90f) % 4 * 2; // between two main directions, the nearer axis
		return vu(FACING[i], AXIS[a] + "  " + Math.round(deg) + "°");
	}

	private static Part[] ping(Minecraft mc, boolean sample) {
		PlayerInfo info = mc.getConnection() == null || mc.player == null ? null : mc.getConnection().getPlayerInfo(mc.player.getUUID());
		if (info == null) return sample ? vu("42", "ms", MINT) : null;
		int ms = info.getLatency();
		return vu(String.valueOf(ms), "ms", ms < 80 ? MINT : ms < 150 ? GOLD : CORAL);
	}

	private static Part[] server(Minecraft mc, boolean sample) {
		ServerData data = mc.getCurrentServer();
		if (data == null || mc.hasSingleplayerServer()) return sample ? vu("play.example.net", "") : null;
		return vu(data.ip, "");
	}

	/** Left and right clicks per second, like "9 | 4 CPS". */
	private static Part[] cpsLine(Minecraft mc, boolean sample) {
		int l = sample && cps(LEFT) == 0 ? 9 : cps(LEFT);
		int r = sample && cps(RIGHT) == 0 ? 4 : cps(RIGHT);
		return new Part[] {Part.value(String.valueOf(l), Ui.BONE), Part.unit("|").after(4), Part.value(String.valueOf(r), Ui.BONE).after(4), Part.unit("CPS")};
	}

	private static Part[] combo(Minecraft mc, boolean sample) {
		int n = sample && combo == 0 ? 3 : combo;
		return vu(String.valueOf(n), n == 1 ? "HIT" : "COMBO", n >= 5 ? GOLD : Ui.BONE);
	}

	private static Part[] reach(Minecraft mc, boolean sample) {
		boolean fresh = reach >= 0 && System.currentTimeMillis() - reachAt < 5000;
		if (!fresh) return sample ? vu("3.00", "BLOCKS") : vu("-", "BLOCKS", Ui.MUTE);
		return vu(String.format(java.util.Locale.ROOT, "%.2f", reach), "BLOCKS", reach > 3.0 ? CORAL : Ui.BONE);
	}

	/** Hunger out of 20 and the saturation behind it, read from the game itself (the server sends both). */
	private static Part[] saturation(Minecraft mc, boolean sample) {
		float sat = 5.6f;
		int food = 18;
		if (mc.player != null) {
			sat = mc.player.getFoodData().getSaturationLevel();
			food = mc.player.getFoodData().getFoodLevel();
		}
		return new Part[] {
				Part.label("FOOD", 0), Part.value(String.valueOf(food), food <= 6 ? CORAL : Ui.BONE).after(4),
				Part.label("SAT", 8), Part.value(String.format(java.util.Locale.ROOT, "%.1f", sat), sat < 1f ? CORAL : sat < 4f ? GOLD : MINT).after(4)};
	}

	// ---------------------------------------------------------------- combo and reach

	private static int combo;
	private static long lastHit;
	private static double reach = -1;
	private static long reachAt;
	private static Entity target;
	private static long attackedAt;
	private static boolean counted;

	/** When the player swings at something (Fabric's attack event): what was hit, and how far away it was. */
	static void onAttack(Minecraft mc, Entity e) {
		long now = System.currentTimeMillis();
		target = e;
		attackedAt = now;
		counted = false;
		if (mc.player != null && mc.hitResult instanceof EntityHitResult hit && hit.getEntity() == e) {
			reach = hit.getLocation().distanceTo(mc.player.getEyePosition());
			reachAt = now;
		}
	}

	/** Once a tick: a hit counts when the target flinches soon after the swing; being hit or a 2 second pause ends it. */
	private static void tickCombat(Player p) {
		long now = System.currentTimeMillis();
		if (!counted && target instanceof LivingEntity le && now - attackedAt < 500 && le.hurtTime > 0 && le.hurtTime >= le.hurtDuration - 1) {
			combo++;
			lastHit = now;
			counted = true;
		}
		if (p.hurtTime > 0 && p.hurtTime >= p.hurtDuration - 1) combo = 0;
		if (combo > 0 && now - lastHit > 2000) combo = 0;
	}

	/** X, Y and Z each with its letter in the accent colour. */
	private static long joinedAt;

	private static Part[] session(Minecraft mc, boolean sample) {
		if (joinedAt == 0) return sample ? vu("1:24:09", "played") : null;
		long s = (System.currentTimeMillis() - joinedAt) / 1000;
		String t = s >= 3600 ? String.format("%d:%02d:%02d", s / 3600, s / 60 % 60, s % 60) : String.format("%d:%02d", s / 60, s % 60);
		return vu(t, "played");
	}

	// ---------------------------------------------------------------- clicks, speed and the session

	static final int LEFT = 0;
	static final int RIGHT = 1;
	@SuppressWarnings("unchecked")
	private static final Deque<Long>[] CLICKS = new Deque[] {new ArrayDeque<Long>(), new ArrayDeque<Long>()};
	private static final boolean[] WAS_DOWN = new boolean[2];
	static double speed;
	private static double lastX = Double.NaN;
	private static double lastZ;

	/** Checked every frame, so even fast clicking between ticks is counted. Only while playing, not in menus. */
	static void pollClicks(Minecraft mc) {
		boolean playing = mc.screen == null && mc.player != null;
		boolean[] down = {playing && mc.options.keyAttack.isDown(), playing && mc.options.keyUse.isDown()};
		long now = System.currentTimeMillis();
		for (int i = 0; i < 2; i++) {
			if (down[i] && !WAS_DOWN[i]) CLICKS[i].addLast(now);
			WAS_DOWN[i] = down[i];
		}
	}

	static int cps(int button) {
		Deque<Long> q = CLICKS[button];
		long cutoff = System.currentTimeMillis() - 1000;
		while (!q.isEmpty() && q.peekFirst() < cutoff) q.pollFirst();
		return q.size();
	}

	/** Once a tick: the session clock, and ground speed from how far the player moved (smoothed so it doesn't flicker). */
	static void tick(Minecraft mc) {
		Player p = mc.player;
		if (mc.level == null) joinedAt = 0;
		else if (joinedAt == 0) joinedAt = System.currentTimeMillis();
		if (p == null) {
			lastX = Double.NaN;
			speed = 0;
			combo = 0;
			return;
		}
		tickCombat(p);
		if (!Double.isNaN(lastX)) {
			double dx = p.getX() - lastX;
			double dz = p.getZ() - lastZ;
			double now = Math.sqrt(dx * dx + dz * dz) * 20;
			speed = now > 100 ? 0 : speed * 0.6 + now * 0.4; // a teleport isn't speed
		}
		lastX = p.getX();
		lastZ = p.getZ();
	}

	// ---------------------------------------------------------------- keystrokes

	/** W, A, S, D, both mouse buttons with their clicks per second, and the space bar; presses ease in and out. */
	static final class Keys extends Module {
		Keys() {
			super("keys", "Keystrokes", "PvP", "WASD", "Your movement keys, mouse buttons and jump light up as you press them.", 0f, 0.7f);
		}

		@Override
		int width(Minecraft mc, boolean sample) {
			return 52;
		}

		@Override
		int height(Minecraft mc, boolean sample) {
			return 63;
		}

		@Override
		void draw(GuiGraphics g, Minecraft mc, boolean sample) {
			Options o = mc.options;
			key(g, mc, 18, 0, 16, 16, "W", o.keyUp.isDown());
			key(g, mc, 0, 18, 16, 16, "A", o.keyLeft.isDown());
			key(g, mc, 18, 18, 16, 16, "S", o.keyDown.isDown());
			key(g, mc, 36, 18, 16, 16, "D", o.keyRight.isDown());
			mouse(g, mc, 0, "LMB", o.keyAttack.isDown());
			mouse(g, mc, 27, "RMB", o.keyUse.isDown());
			float t = box(g, "space", 0, 54, 52, 9, o.keyJump.isDown());
			Ui.round(g, 16, 58, 36, 59, 0.5f, t > 0.5f ? Ui.c(Ui.VOID, 1f) : Ui.c(Ui.BONE, 0.7f));
		}

		/** A key's background; returns how pressed it looks (0 to 1), easing both ways. */
		private static float box(GuiGraphics g, String id, float x, float y, float w, float h, boolean down) {
			float t = MOTION.to("key." + id, down ? 1f : 0f, down ? 30f : 12f);
			Ui.round(g, x, y, x + w, y + h, 3, Ui.c(Ui.VOID, 1f));
			if (t > 0.01f) Ui.round(g, x, y, x + w, y + h, 3, Ui.c(Ui.accent(), t));
			return t;
		}

		private static void key(GuiGraphics g, Minecraft mc, float x, float y, float w, float h, String label, boolean down) {
			float t = box(g, label, x, y, w, h, down);
			Txt.centre(g, mc.font, label, Txt.BOLD, x + w / 2f, y, h, t > 0.5f ? Ui.c(Ui.VOID, 1f) : Ui.c(Ui.BONE, 1f));
		}

		private static void mouse(GuiGraphics g, Minecraft mc, float x, String label, boolean down) {
			float t = box(g, label, x, 36, 25, 16, down);
			Txt.centre(g, mc.font, label, Txt.BOLD, x + 12.5f, 36, 16, t > 0.5f ? Ui.c(Ui.VOID, 1f) : Ui.c(Ui.BONE, 1f));
		}
	}

	// ---------------------------------------------------------------- armour

	/** Each piece of armour and the held item, with durability left; under a tenth it pulses coral. */
	static final class Armour extends Module {
		private static final EquipmentSlot[] SLOTS = {EquipmentSlot.HEAD, EquipmentSlot.CHEST, EquipmentSlot.LEGS, EquipmentSlot.FEET, EquipmentSlot.MAINHAND};
		private static List<ItemStack> samples;

		Armour() {
			super("armour", "Armour Status", "PvP", "ARM", "Your armour and held item, with durability left; it pulses when nearly broken.", 1f, 0.62f);
		}

		private static List<ItemStack> items(Minecraft mc, boolean sample) {
			List<ItemStack> out = new ArrayList<>();
			if (mc.player != null) {
				for (EquipmentSlot slot : SLOTS) {
					ItemStack s = mc.player.getItemBySlot(slot);
					if (!s.isEmpty()) out.add(s);
				}
			}
			if (out.isEmpty() && sample) {
				if (samples == null) {
					samples = List.of(new ItemStack(Items.DIAMOND_HELMET), new ItemStack(Items.DIAMOND_CHESTPLATE),
							new ItemStack(Items.NETHERITE_LEGGINGS), new ItemStack(Items.DIAMOND_BOOTS), new ItemStack(Items.NETHERITE_SWORD));
					samples.get(3).setDamageValue(330);
					samples.get(4).setDamageValue(1950);
				}
				return samples;
			}
			return out;
		}

		private static String label(ItemStack s) {
			if (s.isDamageableItem()) return String.valueOf(s.getMaxDamage() - s.getDamageValue());
			return s.getCount() > 1 ? String.valueOf(s.getCount()) : "";
		}

		@Override
		boolean shows(Minecraft mc) {
			return !items(mc, false).isEmpty();
		}

		@Override
		int width(Minecraft mc, boolean sample) {
			float text = 0;
			for (ItemStack s : items(mc, sample)) text = Math.max(text, Txt.widthF(mc.font, label(s), Txt.BOLD));
			return 3 + 16 + (text > 0 ? 4 + (int) Math.ceil(text) : 0) + 5;
		}

		@Override
		int height(Minecraft mc, boolean sample) {
			return Math.max(1, items(mc, sample).size()) * 18 + 4;
		}

		@Override
		void draw(GuiGraphics g, Minecraft mc, boolean sample) {
			List<ItemStack> list = items(mc, sample);
			panel(g, width(mc, sample), height(mc, sample));
			float pulse = 0.55f + 0.45f * (0.5f + 0.5f * (float) Math.sin(System.currentTimeMillis() / 160.0));
			int y = 3;
			for (ItemStack s : list) {
				g.renderItem(s, 3, y);
				String label = label(s);
				if (!label.isEmpty()) {
					int color = Ui.BONE;
					float alpha = 1f;
					if (s.isDamageableItem() && s.getMaxDamage() > 0) {
						float left = 1f - (float) s.getDamageValue() / s.getMaxDamage();
						color = left > 0.5f ? MINT : left > 0.2f ? GOLD : CORAL;
						if (left < 0.1f) alpha = pulse;
					}
					text(g, mc, label, Txt.BOLD, 23, y, 16, Ui.c(color, alpha));
				}
				y += 18;
			}
		}
	}

	// ---------------------------------------------------------------- items that matter in a fight

	/** How many totems, ender pearls, golden apples and arrows you're carrying; only the ones you have. */
	static final class Counter extends Module {
		private record Kind(ItemStack icon, Item[] items) {
		}

		private static List<Kind> kinds;

		Counter() {
			super("counter", "Item Tracker", "PvP", "TOT", "Totems, ender pearls, golden apples and arrows in your inventory.", 1f, 0.85f);
		}

		private static List<Kind> kinds() {
			if (kinds == null) {
				kinds = List.of(
						new Kind(new ItemStack(Items.TOTEM_OF_UNDYING), new Item[] {Items.TOTEM_OF_UNDYING}),
						new Kind(new ItemStack(Items.ENDER_PEARL), new Item[] {Items.ENDER_PEARL}),
						new Kind(new ItemStack(Items.GOLDEN_APPLE), new Item[] {Items.GOLDEN_APPLE, Items.ENCHANTED_GOLDEN_APPLE}),
						new Kind(new ItemStack(Items.ARROW), new Item[] {Items.ARROW, Items.SPECTRAL_ARROW, Items.TIPPED_ARROW}));
			}
			return kinds;
		}

		/** Each kind with its count; only those you have, or all of them with example counts in the editor. */
		private static List<Object[]> counts(Minecraft mc, boolean sample) {
			List<Object[]> out = new ArrayList<>();
			for (Kind k : kinds()) {
				int n = 0;
				if (mc.player != null) {
					var inv = mc.player.getInventory();
					for (int slot = 0; slot < inv.getContainerSize(); slot++) {
						ItemStack st = inv.getItem(slot);
						for (Item item : k.items()) if (st.is(item)) n += st.getCount();
					}
				}
				if (n > 0) out.add(new Object[] {k.icon(), n});
			}
			if (out.isEmpty() && sample) {
				int[] example = {2, 16, 8, 64};
				int i = 0;
				for (Kind k : kinds()) out.add(new Object[] {k.icon(), example[i++]});
			}
			return out;
		}

		@Override
		boolean shows(Minecraft mc) {
			return !counts(mc, false).isEmpty();
		}

		@Override
		int width(Minecraft mc, boolean sample) {
			float w = 3;
			for (Object[] c : counts(mc, sample)) w += 16 + 2 + Txt.widthF(mc.font, String.valueOf(c[1]), Txt.BOLD) + 6;
			return (int) Math.ceil(w);
		}

		@Override
		int height(Minecraft mc, boolean sample) {
			return 22;
		}

		@Override
		void draw(GuiGraphics g, Minecraft mc, boolean sample) {
			panel(g, width(mc, sample), 22);
			float x = 3;
			for (Object[] c : counts(mc, sample)) {
				g.renderItem((ItemStack) c[0], Math.round(x), 3);
				String n = String.valueOf(c[1]);
				text(g, mc, n, Txt.BOLD, x + 18, 3, 16, Ui.c(Ui.BONE, 1f));
				x += 16 + 2 + Txt.widthF(mc.font, n, Txt.BOLD) + 6;
			}
		}
	}
}
