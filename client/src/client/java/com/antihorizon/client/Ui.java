package com.antihorizon.client;

import com.mojang.blaze3d.platform.NativeImage;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphics;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.client.renderer.texture.DynamicTexture;
import net.minecraft.resources.Identifier;

import java.util.HashMap;
import java.util.Map;
import java.util.function.Supplier;

/** Colours, shapes and motion shared by the menu, the HUD and the HUD editor. The game's palette only. */
final class Ui {
	static final int VOID = 0x070E15;
	static final int BONE = 0xE7ECE5;
	static final int SOFT = 0xB9C6C2;
	static final int MUTE = 0x859C9A;
	/** The one accent colour: the game's gold. */
	static final int GOLD = 0xD8BC8D;

	/** Multiplies every colour drawn, so a whole screen can fade in together. */
	static float fade = 1f;

	private Ui() {
	}

	static int accent() {
		return GOLD;
	}

	/** An RGB colour at an opacity from 0 to 1, times the current fade. */
	static int c(int rgb, float alpha) {
		int a = Math.round(Math.max(0f, Math.min(1f, alpha * fade)) * 255f);
		return (a << 24) | (rgb & 0xFFFFFF);
	}

	/**
	 * How much a HUD piece is enlarged while it draws (its size from the HUD editor). Text and shapes then use the font
	 * and pixel grid for that size, so a piece at 150% is as sharp as one at 100%.
	 */
	static float zoom = 1f;

	/** How many screen pixels one (zoomed) GUI unit covers. */
	static int scale() {
		return Math.max(1, Math.round(Minecraft.getInstance().getWindow().getGuiScale() * zoom));
	}

	/** A GUI position moved to the nearest whole screen pixel. */
	static float snap(float v) {
		int s = scale();
		return Math.round(v * s) / (float) s;
	}

	/** The same colour with its opacity multiplied by k. */
	static int alpha(int argb, float k) {
		int a = Math.round(((argb >>> 24) & 255) * Math.max(0f, Math.min(1f, k)));
		return (a << 24) | (argb & 0xFFFFFF);
	}

	/** A filled rectangle with round corners, drawn in real screen pixels with soft edges. */
	static void round(GuiGraphics g, float x1, float y1, float x2, float y2, float r, int color) {
		shape(g, x1, y1, x2, y2, r, color, true, true);
	}

	/**
	 * Frosted glass, as on the launcher's home screen: a faint light rim, a dark see-through body one screen pixel
	 * inside it, and a brighter hairline along the top edge where the light catches it. Round all the way round.
	 */
	static void glass(GuiGraphics g, float x1, float y1, float x2, float y2, float r) {
		float px = 1f / scale();
		round(g, x1, y1, x2, y2, r, c(BONE, 0.16f));
		round(g, x1 + px, y1 + px, x2 - px, y2 - px, Math.max(0f, r - px), c(VOID, 0.62f));
		float inset = Math.min(r, (x2 - x1) / 2f);
		hline(g, x1 + inset, x2 - inset, y1 + px, c(BONE, 0.35f));
	}

	/** Like round(), but only the bottom corners are rounded (for a strip along the bottom of a card). */
	static void roundBottom(GuiGraphics g, float x1, float y1, float x2, float y2, float r, int color) {
		shape(g, x1, y1, x2, y2, r, color, false, true);
	}

	/** A line one screen pixel thick, across from x1 to x2 at y. */
	static void hline(GuiGraphics g, float x1, float x2, float y, int color) {
		int s = scale();
		pixels(g, s);
		int py = Math.round(y * s);
		g.fill(Math.round(x1 * s), py, Math.round(x2 * s), py + 1, color);
		g.pose().popMatrix();
	}

	/** A line one screen pixel thick, down from y1 to y2 at x. */
	static void vline(GuiGraphics g, float x, float y1, float y2, int color) {
		int s = scale();
		pixels(g, s);
		int px = Math.round(x * s);
		g.fill(px, Math.round(y1 * s), px + 1, Math.round(y2 * s), color);
		g.pose().popMatrix();
	}

	/** Pushes a transform where one unit is one screen pixel. Pop it when done. */
	static void pixels(GuiGraphics g, int s) {
		g.pose().pushMatrix();
		g.pose().scale(1f / s, 1f / s);
	}

	/**
	 * A box with round corners in at most seven quads: the corners are quarters of one baked soft-edged disc, the rest
	 * plain fills. (Drawing the corners a pixel at a time took over a hundred fills a box, which made the HUD lag.)
	 */
	private static void shape(GuiGraphics g, float x1, float y1, float x2, float y2, float r, int color, boolean top, boolean bottom) {
		int s = scale();
		int px1 = Math.round(x1 * s);
		int py1 = Math.round(y1 * s);
		int px2 = Math.round(x2 * s);
		int py2 = Math.round(y2 * s);
		if (px2 <= px1 || py2 <= py1) return;
		int rr = Math.round(r * s);
		rr = Math.min(rr, Math.min((px2 - px1) / 2, top && bottom ? (py2 - py1) / 2 : py2 - py1));
		pixels(g, s);
		if (rr <= 0) {
			g.fill(px1, py1, px2, py2, color);
			g.pose().popMatrix();
			return;
		}
		int bodyTop = top ? py1 + rr : py1;
		int bodyBottom = bottom ? py2 - rr : py2;
		if (bodyBottom > bodyTop) g.fill(px1, bodyTop, px2, bodyBottom, color);
		Identifier disc = disc(rr);
		int d = rr * 2;
		if (top) {
			g.blit(RenderPipelines.GUI_TEXTURED, disc, px1, py1, 0, 0, rr, rr, d, d, color);
			g.blit(RenderPipelines.GUI_TEXTURED, disc, px2 - rr, py1, rr, 0, rr, rr, d, d, color);
			if (px2 - rr > px1 + rr) g.fill(px1 + rr, py1, px2 - rr, py1 + rr, color);
		}
		if (bottom) {
			g.blit(RenderPipelines.GUI_TEXTURED, disc, px1, py2 - rr, 0, rr, rr, rr, d, d, color);
			g.blit(RenderPipelines.GUI_TEXTURED, disc, px2 - rr, py2 - rr, rr, rr, rr, rr, d, d, color);
			if (px2 - rr > px1 + rr) g.fill(px1 + rr, py2 - rr, px2 - rr, py2, color);
		}
		g.pose().popMatrix();
	}

	/** A white disc r screen pixels in radius with soft edges, mirrored from one corner's coverage. */
	private static Identifier disc(int r) {
		return mask("disc_" + r, r * 2, r * 2, () -> {
			float[][] q = corner(r);
			float[][] c = new float[r * 2][r * 2];
			for (int j = 0; j < r; j++) {
				for (int i = 0; i < r; i++) {
					c[j][i] = q[j][i];
					c[j][r * 2 - 1 - i] = q[j][i];
					c[r * 2 - 1 - j][i] = q[j][i];
					c[r * 2 - 1 - j][r * 2 - 1 - i] = q[j][i];
				}
			}
			return c;
		});
	}

	private static final Map<String, Identifier> MASKS = new HashMap<>();

	/**
	 * A white shape baked once into a small texture (coverage[y][x], 0 to 1, as its opacity), then drawn tinted to any
	 * colour as a single quad. Used for corners and icons.
	 */
	static Identifier mask(String key, int w, int h, Supplier<float[][]> coverage) {
		Identifier id = MASKS.get(key);
		if (id != null) return id;
		float[][] c = coverage.get();
		NativeImage img = new NativeImage(w, h, true);
		for (int y = 0; y < h; y++) {
			for (int x = 0; x < w; x++) {
				int a = Math.round(Math.max(0f, Math.min(1f, c[y][x])) * 255f);
				img.setPixel(x, y, (a << 24) | 0xFFFFFF);
			}
		}
		id = Identifier.fromNamespaceAndPath("antihorizon", "mask/" + key);
		Minecraft.getInstance().getTextureManager().register(id, new DynamicTexture(() -> "Anti-Horizon " + key, img));
		MASKS.put(key, id);
		return id;
	}

	/** Draws a baked mask at x, y (GUI units, snapped to screen pixels), w by h screen pixels, in a colour. */
	static void drawMask(GuiGraphics g, Identifier mask, float x, float y, int w, int h, int color) {
		int s = scale();
		pixels(g, s);
		g.blit(RenderPipelines.GUI_TEXTURED, mask, Math.round(x * s), Math.round(y * s), 0, 0, w, h, w, h, color);
		g.pose().popMatrix();
	}

	private static final Map<Integer, float[][]> CORNERS = new HashMap<>();

	/** How much of each pixel in a top-left corner of radius r is inside the curve, sampled 5 by 5. */
	private static float[][] corner(int r) {
		return CORNERS.computeIfAbsent(r, k -> {
			float[][] cover = new float[r][r];
			for (int j = 0; j < r; j++) {
				for (int i = 0; i < r; i++) {
					int in = 0;
					for (int a = 0; a < 5; a++) {
						for (int b = 0; b < 5; b++) {
							float dx = r - (i + (a + 0.5f) / 5f);
							float dy = r - (j + (b + 0.5f) / 5f);
							if (dx * dx + dy * dy <= r * r) in++;
						}
					}
					cover[j][i] = in / 25f;
				}
			}
			return cover;
		});
	}

	static float ease(float t) {
		float k = 1f - Math.max(0f, Math.min(1f, t));
		return 1f - k * k * k;
	}

	/** Smoothly eases named values towards their targets, frame-rate independent. Instant when animations are off. */
	static final class Motion {
		private final Map<String, Float> values = new HashMap<>();
		private long last = System.nanoTime();
		private float dt;

		void tick() {
			long now = System.nanoTime();
			dt = Math.min(0.1f, (now - last) / 1_000_000_000f);
			last = now;
		}

		float to(String key, float target, float speed) {
			if (!Config.animations) {
				values.put(key, target);
				return target;
			}
			float v = values.getOrDefault(key, target);
			v += (target - v) * (1f - (float) Math.exp(-speed * dt));
			values.put(key, v);
			return v;
		}

		void set(String key, float value) {
			values.put(key, value);
		}
	}
}
