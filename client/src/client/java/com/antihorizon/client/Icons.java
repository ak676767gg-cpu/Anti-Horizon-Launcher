package com.antihorizon.client;

import net.minecraft.client.gui.GuiGraphics;
import net.minecraft.resources.Identifier;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * Small line icons, eight GUI units square. Each is described as shapes (lines, circles, rounded boxes), turned into
 * soft-edged screen pixels once per GUI scale, and kept, so they are as smooth as the text at any scale.
 */
final class Icons {
	/** Distance from a point to a shape's edge, in GUI units: negative inside, positive outside. */
	private interface Shape {
		float dist(float x, float y);
	}

	private static final float SIZE = 8f;
	/** Each icon's baked texture, by GUI scale then name. */
	private static final Map<Integer, Map<String, Identifier>> BAKED = new HashMap<>();

	private Icons() {
	}

	static void draw(GuiGraphics g, String name, float x, float y, int color) {
		int s = Ui.scale();
		int n = (int) Math.ceil(SIZE * s);
		Identifier tex = BAKED.computeIfAbsent(s, k -> new HashMap<>())
				.computeIfAbsent(name, k -> Ui.mask("icon_" + name + "_" + s, n, n, () -> bake(shapes(name), s)));
		Ui.drawMask(g, tex, Ui.snap(x), Ui.snap(y), n, n, color);
	}

	private static List<Shape> shapes(String name) {
		List<Shape> s = new ArrayList<>();
		switch (name) {
			case "grid" -> {
				s.add(box(0.5f, 0.5f, 3.6f, 3.6f, 0.9f));
				s.add(box(4.4f, 0.5f, 7.5f, 3.6f, 0.9f));
				s.add(box(0.5f, 4.4f, 3.6f, 7.5f, 0.9f));
				s.add(box(4.4f, 4.4f, 7.5f, 7.5f, 0.9f));
			}
			case "key" -> {
				s.add(frame(0.4f, 1.4f, 7.6f, 6.6f, 1.3f, 0.8f));
				s.add(disc(2.3f, 3.4f, 0.55f));
				s.add(disc(4f, 3.4f, 0.55f));
				s.add(disc(5.7f, 3.4f, 0.55f));
				s.add(line(2.7f, 5.0f, 5.3f, 5.0f, 0.8f));
			}
			case "brush" -> {
				s.add(line(7.2f, 0.8f, 4.3f, 3.7f, 1.0f));
				s.add(line(3.5f, 4.5f, 1.6f, 6.4f, 2.1f));
			}
			case "gear" -> {
				s.add(ring(4f, 4f, 2.2f, 1.3f));
				for (int k = 0; k < 8; k++) {
					double a = Math.PI / 4 * k;
					float c = (float) Math.cos(a);
					float n = (float) Math.sin(a);
					s.add(line(4f + 2.6f * c, 4f + 2.6f * n, 4f + 3.3f * c, 4f + 3.3f * n, 1.3f));
				}
			}
			case "hud" -> {
				s.add(frame(0.4f, 0.8f, 7.6f, 7.2f, 1.3f, 0.8f));
				s.add(box(1.9f, 2.3f, 4.4f, 3.9f, 0.5f));
			}
			case "search" -> {
				s.add(ring(3.3f, 3.3f, 2.5f, 0.85f));
				s.add(line(5.2f, 5.2f, 7.3f, 7.3f, 1.0f));
			}
			case "close" -> {
				s.add(line(1.3f, 1.3f, 6.7f, 6.7f, 0.9f));
				s.add(line(6.7f, 1.3f, 1.3f, 6.7f, 0.9f));
			}
			case "left" -> {
				s.add(line(5.2f, 1.2f, 2.4f, 4f, 0.9f));
				s.add(line(2.4f, 4f, 5.2f, 6.8f, 0.9f));
			}
			case "right" -> {
				s.add(line(2.8f, 1.2f, 5.6f, 4f, 0.9f));
				s.add(line(5.6f, 4f, 2.8f, 6.8f, 0.9f));
			}
			case "sun" -> {
				s.add(disc(4f, 4f, 1.7f));
				for (int k = 0; k < 8; k++) {
					double a = Math.PI / 4 * k;
					float c = (float) Math.cos(a);
					float n = (float) Math.sin(a);
					s.add(line(4f + 2.7f * c, 4f + 2.7f * n, 4f + 3.6f * c, 4f + 3.6f * n, 0.75f));
				}
			}
			case "moon" -> {
				// A crescent: a disc with a second, offset disc taken out of it.
				Shape outer = disc(4f, 4f, 3.3f);
				Shape bite = disc(5.6f, 2.9f, 2.7f);
				s.add((x, y) -> Math.max(outer.dist(x, y), -bite.dist(x, y)));
			}
			case "person" -> {
				s.add(disc(4f, 2.5f, 1.7f));
				s.add(box(1.2f, 5.1f, 6.8f, 9.5f, 2.4f));
			}
			case "people" -> {
				s.add(disc(5.4f, 2.2f, 1.35f));
				s.add(box(3.4f, 4.6f, 8.2f, 9.4f, 2.0f));
				s.add(disc(2.9f, 3.1f, 1.55f));
				s.add(box(0.1f, 5.6f, 5.7f, 10f, 2.3f));
			}
			case "home" -> {
				s.add(line(0.7f, 4.1f, 4f, 0.9f, 1.0f));
				s.add(line(4f, 0.9f, 7.3f, 4.1f, 1.0f));
				s.add(frame(1.9f, 3.6f, 6.1f, 7.5f, 0.5f, 0.9f));
			}
			case "globe" -> {
				s.add(ring(4f, 4f, 3.3f, 0.9f));
				s.add(line(0.9f, 4f, 7.1f, 4f, 0.8f));
				s.add((x, y) -> Math.abs((float) Math.hypot((x - 4f) / 1.4f, (y - 4f) / 3.3f) - 1f) * 1.4f - 0.45f);
			}
			case "power" -> {
				Shape arc = ring(4f, 4.4f, 2.9f, 0.9f);
				Shape gap = box(2.7f, -1f, 5.3f, 3.2f, 0f);
				s.add((x, y) -> Math.max(arc.dist(x, y), -gap.dist(x, y)));
				s.add(line(4f, 0.6f, 4f, 3.9f, 0.9f));
			}
			default -> {
			}
		}
		return s;
	}

	/** How much of each screen pixel of the icon the shapes cover, with a one-pixel soft edge. */
	private static float[][] bake(List<Shape> shapes, int s) {
		int n = (int) Math.ceil(SIZE * s);
		float[][] cover = new float[n][n];
		for (int py = 0; py < n; py++) {
			for (int px = 0; px < n; px++) {
				float u = (px + 0.5f) / s;
				float v = (py + 0.5f) / s;
				float d = Float.MAX_VALUE;
				for (Shape sh : shapes) d = Math.min(d, sh.dist(u, v));
				cover[py][px] = Math.max(0f, Math.min(1f, 0.5f - d * s));
			}
		}
		return cover;
	}

	// ---------------------------------------------------------------- shapes

	private static Shape line(float ax, float ay, float bx, float by, float width) {
		return (x, y) -> {
			float dx = bx - ax;
			float dy = by - ay;
			float t = Math.max(0f, Math.min(1f, ((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)));
			return (float) Math.hypot(x - ax - t * dx, y - ay - t * dy) - width / 2f;
		};
	}

	private static Shape disc(float cx, float cy, float r) {
		return (x, y) -> (float) Math.hypot(x - cx, y - cy) - r;
	}

	private static Shape ring(float cx, float cy, float r, float width) {
		return (x, y) -> Math.abs((float) Math.hypot(x - cx, y - cy) - r) - width / 2f;
	}

	private static Shape box(float x1, float y1, float x2, float y2, float r) {
		float cx = (x1 + x2) / 2f;
		float cy = (y1 + y2) / 2f;
		float hx = (x2 - x1) / 2f - r;
		float hy = (y2 - y1) / 2f - r;
		return (x, y) -> {
			float qx = Math.abs(x - cx) - hx;
			float qy = Math.abs(y - cy) - hy;
			float outside = (float) Math.hypot(Math.max(qx, 0f), Math.max(qy, 0f));
			return outside + Math.min(Math.max(qx, qy), 0f) - r;
		};
	}

	private static Shape frame(float x1, float y1, float x2, float y2, float r, float width) {
		Shape b = box(x1, y1, x2, y2, r);
		return (x, y) -> Math.abs(b.dist(x, y)) - width / 2f;
	}
}
