package com.antihorizon.client;

import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphics;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.FontDescription;
import net.minecraft.resources.Identifier;


/**
 * Smooth text in Chakra Petch: squared letters with clipped corners, like a spacecraft display, open and wide enough
 * to read at HUD sizes. Medium for text, SemiBold for values, Bold for titles.
 *
 * Minecraft draws font textures without blending between pixels, so a smooth font only stays smooth when each of its
 * pixels lands on exactly one screen pixel. So every face exists once per GUI scale (assets/antihorizon/font/name_N),
 * rendered at that scale, and every position is snapped to whole screen pixels.
 */
final class Txt {
	/** A typeface at one size. cap is its capital height in GUI units, for centring text in a box. */
	static final class Face {
		private final String name;
		private final float cap;
		/** The font for each GUI scale, made the first time it's needed. */
		private final FontDescription[] fonts = new FontDescription[MAX_SCALE + 1];

		Face(String name, float cap) {
			this.name = name;
			this.cap = cap;
		}

		String name() {
			return name;
		}

		float cap() {
			return cap;
		}
	}

	static final Face UI = new Face("ui", 6.65f);
	static final Face BOLD = new Face("bold", 6.65f);
	static final Face SMALL = new Face("small", 5.95f);
	static final Face TITLE = new Face("title", 9.8f);
	static final Face BRAND = new Face("brand", 7.7f);

	/** Minecraft puts the baseline this far below the y a string is drawn at, whatever the font. */
	private static final float BASELINE = 7f;
	private static final int MAX_SCALE = 8;

	private Txt() {
	}

	private static FontDescription font(Face f) {
		int s = Math.max(1, Math.min(MAX_SCALE, Ui.scale()));
		FontDescription d = f.fonts[s];
		if (d == null) d = f.fonts[s] = new FontDescription.Resource(Identifier.fromNamespaceAndPath("antihorizon", f.name + "_" + s));
		return d;
	}

	static Component of(String s, Face f) {
		FontDescription d = font(f);
		return Component.literal(s).withStyle(st -> st.withFont(d));
	}

	/** Exact width in GUI units (can be fractional). */
	static float widthF(Font font, String s, Face f) {
		return font.getSplitter().stringWidth(of(s, f));
	}

	static int width(Font font, String s, Face f) {
		return (int) Math.ceil(widthF(font, s, f));
	}

	/** Draws with the top of the line box at y. No shadow: it looks muddy on a smooth font. */
	static void draw(GuiGraphics g, Font font, String s, Face f, float x, float y, int color) {
		float sx = Ui.snap(x);
		float sy = Ui.snap(y);
		if (sx == (int) sx && sy == (int) sy) {
			g.drawString(font, of(s, f), (int) sx, (int) sy, color, false);
			return;
		}
		g.pose().pushMatrix();
		g.pose().translate(sx, sy);
		g.drawString(font, of(s, f), 0, 0, color, false);
		g.pose().popMatrix();
	}

	/** Draws with the baseline (the line capitals sit on) at y. */
	static void base(GuiGraphics g, Font font, String s, Face f, float x, float baseline, int color) {
		draw(g, font, s, f, x, baseline - BASELINE, color);
	}

	/** Draws so the capitals sit in the vertical middle of a box from top to top + h. */
	static void middle(GuiGraphics g, Font font, String s, Face f, float x, float top, float h, int color) {
		base(g, font, s, f, x, top + (h + f.cap()) / 2f, color);
	}

	/** Centred on cx, in the vertical middle of a box. */
	static void centre(GuiGraphics g, Font font, String s, Face f, float cx, float top, float h, int color) {
		middle(g, font, s, f, cx - widthF(font, s, f) / 2f, top, h, color);
	}

	/** The text, shortened with an ellipsis if it is wider than maxW. */
	static String fit(Font font, String s, Face f, float maxW) {
		if (widthF(font, s, f) <= maxW) return s;
		String t = s;
		while (t.length() > 1 && widthF(font, t + "...", f) > maxW) t = t.substring(0, t.length() - 1);
		return t.stripTrailing() + "...";
	}
}
