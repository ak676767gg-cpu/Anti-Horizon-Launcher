package com.antihorizon.client;

import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexConsumer;
import com.mojang.math.Axis;
import net.minecraft.client.model.player.PlayerModel;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.entity.RenderLayerParent;
import net.minecraft.client.renderer.entity.layers.RenderLayer;
import net.minecraft.client.renderer.entity.state.AvatarRenderState;
import net.minecraft.client.renderer.rendertype.RenderType;
import net.minecraft.client.renderer.rendertype.RenderTypes;
import net.minecraft.client.renderer.texture.OverlayTexture;
import net.minecraft.core.component.DataComponents;
import net.minecraft.resources.Identifier;

/**
 * Draws a player's Anti-Horizon wings and head piece. Everything is plain textured quads handed to the game's own
 * renderer, in the player model's space (1 unit = 1 texel of the skin), so they move with the body and the head.
 */
public final class CosmeticsLayer extends RenderLayer<AvatarRenderState, PlayerModel> {
	private static final int FULL_BRIGHT = 0xF000F0;
	private static final float DEG = (float) Math.PI / 180f;

	public CosmeticsLayer(RenderLayerParent<AvatarRenderState, PlayerModel> parent) {
		super(parent);
	}

	@Override
	public void submit(PoseStack pose, SubmitNodeCollector nodes, int light, AvatarRenderState state, float yRot, float xRot) {
		if (state.isInvisible) return;
		Cosmetics.Look look = Cosmetics.ofEntity(state.id);
		if (look == null) return;
		// Elytra replace the cape and the wings while worn (the halo floats above a helmet).
		boolean elytra = state.chestEquipment.has(DataComponents.GLIDER);
		if (look.cape() != null && state.showCape && !elytra) cape(pose, nodes, light, state, look.cape());
		if (look.wings() != null && !state.chestEquipment.has(DataComponents.GLIDER)) wings(pose, nodes, light, state, look.wings());
		if (look.head() != null) head(pose, nodes, light, state, look.head());
	}

	// ---------------------------------------------------------------- cape

	private static final int COLS = 24, ROWS = 38;
	/** Seconds for the disk to turn once around the hole. */
	private static final float TURN_SECONDS = 8f;

	/**
	 * The cape, hung and swung exactly as Minecraft's own (PlayerCapeModel: 2 pixels behind the body, then its lean and
	 * flap). Its outside is the sharp face picture, dimmed where the disk is, with a copy turning around the black hole
	 * added on top as light, over the disk only: the disk goes round and round while the hole, the bright ring right
	 * around it, the jet and the sky stay where they are. Added light can't put dark gaps in the disk, and at any
	 * moment the two add up to the picture's own brightness.
	 */
	private void cape(PoseStack pose, SubmitNodeCollector nodes, int light, AvatarRenderState s, String id) {
		Cosmetics.Flow flow = Cosmetics.flow(id);
		Cosmetics.Clip clip = Cosmetics.clip(id);
		Identifier face = Cosmetics.face(id, s.distanceToCameraSq > 36);
		if ((flow == null && clip == null) || face == null) return;
		Identifier sheet = Cosmetics.texture("cape", id);
		pose.pushPose();
		if (!s.chestEquipment.isEmpty()) pose.translate(0f, -0.053125f, 0.06875f); // over a chestplate, like Minecraft's
		getParentModel().body.translateAndRotate(pose);
		pose.translate(0f, 0f, 0.125f);
		pose.mulPose(Axis.XP.rotationDegrees(6f + s.capeLean / 2f + s.capeFlap));
		pose.mulPose(Axis.ZP.rotationDegrees(s.capeLean2 / 2f));
		pose.mulPose(Axis.YP.rotationDegrees(180f - s.capeLean2 / 2f));
		pose.scale(1 / 16f, 1 / 16f, 1 / 16f);
		double now = (System.nanoTime() / 1_000_000L % 3_600_000L) / 1000.0;
		float turn = flow == null ? 0f : (float) (2 * Math.PI * ((now / TURN_SECONDS) % 1.0)) * flow.spin();
		nodes.submitCustomGeometry(pose, RenderTypes.entitySolid(sheet), (p, vc) -> capeShell(p, vc, light));
		if (clip != null) {
			// A live cape: the clip's frame now, and the next one faded in over it; its still until the clip is here.
			ClipCape.Frame fr = ClipCape.frame(id, clip);
			Identifier a = fr != null ? fr.a() : face;
			nodes.submitCustomGeometry(pose, RenderTypes.entitySolid(a), (p, vc) -> plainFace(p, vc, light, -1f, -1));
			if (fr != null && fr.b() != null && fr.t() > 0.01f) {
				int over = (Math.round(fr.t() * 255f) << 24) | 0xFFFFFF;
				nodes.submitCustomGeometry(pose, RenderTypes.entityTranslucent(fr.b()), (p, vc) -> plainFace(p, vc, light, -1.02f, over));
			}
			pose.popPose();
			return;
		}
		nodes.submitCustomGeometry(pose, RenderTypes.entitySolid(face), (p, vc) -> capeFace(p, vc, light, flow, turn, -1f, false));
		nodes.submitCustomGeometry(pose, RenderTypes.eyes(face), (p, vc) -> capeFace(p, vc, FULL_BRIGHT, flow, turn, -1.02f, true));
		pose.popPose();
	}

	/** The whole outside, once, at depth z and this colour (a live cape's frame). */
	private static void plainFace(PoseStack.Pose p, VertexConsumer vc, int light, float z, int colour) {
		vertex(p, vc, -5, 0, z, 0, 0, light, colour, 0, 0, -1);
		vertex(p, vc, 5, 0, z, 1, 0, light, colour, 0, 0, -1);
		vertex(p, vc, 5, 16, z, 1, 1, light, colour, 0, 0, -1);
		vertex(p, vc, -5, 16, z, 0, 1, light, colour, 0, 0, -1);
		vertex(p, vc, -5, 0, z, 0, 0, light, colour, 0, 0, 1);
		vertex(p, vc, -5, 16, z, 0, 1, light, colour, 0, 0, 1);
		vertex(p, vc, 5, 16, z, 1, 1, light, colour, 0, 0, 1);
		vertex(p, vc, 5, 0, z, 1, 0, light, colour, 0, 0, 1);
	}

	/** The inside and the edges, still, from Minecraft's cape layout (64 x 32). The box is (-5, 0, -1) to (5, 16, 0). */
	private static void capeShell(PoseStack.Pose p, VertexConsumer vc, int light) {
		float u = 1 / 64f, v = 1 / 32f;
		both(p, vc, light, 5, 0, 0, -5, 0, 0, -5, 16, 0, 5, 16, 0, 12 * u, 1 * v, 22 * u, 17 * v, 0, 0, 1);       // inside
		both(p, vc, light, -5, 0, -1, -5, 0, 0, -5, 16, 0, -5, 16, -1, 0, 1 * v, 1 * u, 17 * v, -1, 0, 0);       // sides
		both(p, vc, light, 5, 0, 0, 5, 0, -1, 5, 16, -1, 5, 16, 0, 11 * u, 1 * v, 12 * u, 17 * v, 1, 0, 0);
		both(p, vc, light, -5, 0, 0, 5, 0, 0, 5, 0, -1, -5, 0, -1, 1 * u, 0, 11 * u, 1 * v, 0, -1, 0);           // top
		both(p, vc, light, -5, 16, -1, 5, 16, -1, 5, 16, 0, -5, 16, 0, 11 * u, 0, 21 * u, 1 * v, 0, 1, 0);       // bottom
	}

	/**
	 * The outside at depth z on a grid: the still picture dimmed where the disk is, or (turning) the turned copy, as
	 * light, only where the disk is. Seen from behind, the face's left edge (u = 0) is the player's left: x = -5 here.
	 */
	private static void capeFace(PoseStack.Pose p, VertexConsumer vc, int light, Cosmetics.Flow f, float turn, float z, boolean turning) {
		float[][] at = new float[(COLS + 1) * (ROWS + 1)][];
		for (int j = 0; j <= ROWS; j++)
			for (int i = 0; i <= COLS; i++) at[j * (COLS + 1) + i] = disk(f, i / (float) COLS, j / (float) ROWS, turn);
		for (int j = 0; j < ROWS; j++) {
			for (int i = 0; i < COLS; i++) {
				float[] a = at[j * (COLS + 1) + i], b = at[j * (COLS + 1) + i + 1], c = at[(j + 1) * (COLS + 1) + i + 1], d = at[(j + 1) * (COLS + 1) + i];
				if (turning && a[2] + b[2] + c[2] + d[2] == 0) continue; // nothing of the disk here
				float x0 = -5 + 10f * i / COLS, x1 = -5 + 10f * (i + 1) / COLS, y0 = 16f * j / ROWS, y1 = 16f * (j + 1) / ROWS;
				for (int side = 0; side < 2; side++) {
					float[][] order = side == 0 ? new float[][] {a, b, c, d} : new float[][] {a, d, c, b};
					float[][] xy = side == 0 ? new float[][] {{x0, y0}, {x1, y0}, {x1, y1}, {x0, y1}} : new float[][] {{x0, y0}, {x0, y1}, {x1, y1}, {x1, y0}};
					for (int k = 0; k < 4; k++) {
						float m = order[k][2] * f.mix();
						int g = Math.round((turning ? m : 1f - m) * 255f);
						int colour = 0xFF000000 | g << 16 | g << 8 | g;
						float u = turning ? order[k][0] : (xy[k][0] + 5) / 10f, v = turning ? order[k][1] : xy[k][1] / 16f;
						vertex(p, vc, xy[k][0], xy[k][1], z, u, v, light, colour, 0, 0, side == 0 ? -1 : 1);
					}
				}
			}
		}
	}

	/**
	 * For the point (s, t) of the cape's face: where on the picture the turning disk shows there, and how much of it
	 * (0 at the hole and its ring, and out where the disk ends). The point is taken into the disk's own plane (untilted
	 * and unflattened), turned around the hole and brought back. The same sums are in the launcher's preview3d.js.
	 */
	static float[] disk(Cosmetics.Flow f, float s, float t, float turn) {
		float aspect = 1.6f;
		float dx = s - f.x(), dy = (t - f.y()) * aspect;
		float tilt = f.tilt() * DEG, ca = (float) Math.cos(tilt), sa = (float) Math.sin(tilt);
		float ex = dx * ca + dy * sa, ey = (-dx * sa + dy * ca) / f.ratio();
		float r = (float) Math.sqrt(ex * ex + ey * ey);
		// The hole and its bright ring stay still: measured on the picture, so that part is round.
		float inner = smooth(((float) Math.sqrt(dx * dx + dy * dy) - f.hole() * 1.15f) / (f.hole() * 0.45f));
		float outer = 1f - smooth((r - f.outer() * 0.6f) / (f.outer() * 0.4f));
		float c = (float) Math.cos(turn), sn = (float) Math.sin(turn);
		float rx = ex * c - ey * sn, ry = (ex * sn + ey * c) * f.ratio();
		float u = f.x() + rx * ca - ry * sa, v = f.y() + (rx * sa + ry * ca) / aspect;
		// Fade out where the turned copy would show something from off the edge of the picture.
		float edge = Math.min(Math.min(u, 1 - u), Math.min(v, 1 - v));
		return new float[] {u, v, inner * outer * smooth(edge / 0.22f)};
	}

	private static float smooth(float x) {
		x = Math.max(0f, Math.min(1f, x));
		return x * x * (3 - 2 * x);
	}

	// ---------------------------------------------------------------- wings

	/**
	 * Jointed wings in Minecraft's pixel style (after Fantastic Wings): three pieces per wing, shoulder, forearm and
	 * hand, each a sheet of feathers hanging from its bone. Per piece: width and height in skin pixels, then its place
	 * in the 64x64 atlas (x, width, height in texels). Same numbers in the art script and the launcher's preview3d.js.
	 */
	private static final float[][] PIECES = {{6, 12, 0, 12, 24}, {8, 15, 12, 16, 30}, {11, 18, 28, 22, 36}};
	/** Each piece's turn back and lift up (degrees) from the one before: standing, gliding, crouching. */
	private static final float[][] REST = {{30, 64}, {10, 20}, {12, -28}};
	private static final float[][] SPREAD = {{12, 8}, {4, 2}, {6, -6}};
	private static final float[][] TUCK = {{72, 14}, {36, -12}, {28, -40}};

	private void wings(PoseStack pose, SubmitNodeCollector nodes, int light, AvatarRenderState s, String id) {
		float t = s.ageInTicks;
		float moving = Math.min(1f, s.walkAnimationSpeed * 1.5f);
		boolean gliding = s.fallFlyingTimeInTicks > 0;
		// Upright at rest, a little more open on the move, spread wide in the air, folded in when crouching.
		float open = gliding ? 1f : 0.3f * moving;
		float rate = gliding ? 0.42f : 0.07f + 0.14f * moving;
		float beat = gliding ? 16f : 3f + 7f * moving;
		RenderType type = RenderTypes.entityCutout(Cosmetics.texture("wings", id));

		pose.pushPose();
		getParentModel().body.translateAndRotate(pose);
		pose.scale(1 / 16f, 1 / 16f, 1 / 16f);
		for (int side = -1; side <= 1; side += 2) {
			pose.pushPose();
			pose.translate(side * 1.5f, 1.0f, 2.4f); // the shoulder blades, just off the back (+z is behind)
			for (int k = 0; k < 3; k++) {
				float yaw = s.isCrouching ? TUCK[k][0] : REST[k][0] + (SPREAD[k][0] - REST[k][0]) * open;
				float roll = s.isCrouching ? TUCK[k][1] : REST[k][1] + (SPREAD[k][1] - REST[k][1]) * open;
				// The beat runs out from the shoulder to the tip, each piece a little behind the last.
				roll += (float) Math.sin(t * rate - k * 0.7f) * beat * (k == 0 ? 1f : 0.7f);
				if (k > 0) pose.translate(side * PIECES[k - 1][0], 0, 0);
				pose.mulPose(Axis.YP.rotationDegrees(-side * yaw)); // back
				pose.mulPose(Axis.ZP.rotationDegrees(-side * roll)); // up (-y is up)
				float[] pc = PIECES[k];
				float x = side * pc[0], top = -0.5f, bottom = pc[1];
				float u0 = pc[2] / 64f, u1 = (pc[2] + pc[3]) / 64f, v1 = pc[4] / 64f;
				nodes.submitCustomGeometry(pose, type, (p, vc) -> both(p, vc, light, 0, top, 0, x, top, 0, x, bottom, 0, 0, bottom, 0, u0, 0, u1, v1, 0, 0, 1));
			}
			pose.popPose();
		}
		pose.popPose();
	}

	// ---------------------------------------------------------------- head

	private void head(PoseStack pose, SubmitNodeCollector nodes, int light, AvatarRenderState s, String id) {
		float t = s.ageInTicks;
		Identifier tex = Cosmetics.texture("head", id);
		pose.pushPose();
		getParentModel().head.translateAndRotate(pose);
		pose.scale(1 / 16f, 1 / 16f, 1 / 16f);
		switch (id) {
			case "halo" -> {
				// A glowing gold ring floating above the head, bobbing gently.
				pose.translate(0, -11.6f + (float) Math.sin(t * 0.08f) * 0.35f, 0);
				nodes.submitCustomGeometry(pose, RenderTypes.eyes(tex), (p, vc) -> torus(p, vc, 4.4f, 0.55f));
			}
			case "accretion" -> {
				// A black hole's disk around the head: tilted, turning slowly, glowing.
				pose.translate(0, -4.5f, 0);
				pose.mulPose(Axis.XP.rotationDegrees(18f));
				pose.mulPose(Axis.ZP.rotationDegrees(-9f));
				pose.mulPose(Axis.YP.rotationDegrees(t * 1.8f));
				nodes.submitCustomGeometry(pose, RenderTypes.eyes(tex), (p, vc) -> disk(p, vc, 11f));
			}
			default -> {
			}
		}
		pose.popPose();
	}

	/** A ring lying flat (in x-z), radius r around the middle, tube radius a, textured across the tube. */
	private static void torus(PoseStack.Pose p, VertexConsumer vc, float r, float a) {
		int n = 40, m = 8;
		for (int i = 0; i < n; i++) {
			for (int j = 0; j < m; j++) {
				float[] q0 = ring(r, a, i, j, n, m), q1 = ring(r, a, i + 1, j, n, m), q2 = ring(r, a, i + 1, j + 1, n, m), q3 = ring(r, a, i, j + 1, n, m);
				float u0 = i / (float) n, u1 = (i + 1) / (float) n, v0 = j / (float) m, v1 = (j + 1) / (float) m;
				vertex(p, vc, q0[0], q0[1], q0[2], u0, v0, FULL_BRIGHT, q0[3], q0[4], q0[5]);
				vertex(p, vc, q1[0], q1[1], q1[2], u1, v0, FULL_BRIGHT, q1[3], q1[4], q1[5]);
				vertex(p, vc, q2[0], q2[1], q2[2], u1, v1, FULL_BRIGHT, q2[3], q2[4], q2[5]);
				vertex(p, vc, q3[0], q3[1], q3[2], u0, v1, FULL_BRIGHT, q3[3], q3[4], q3[5]);
			}
		}
	}

	private static float[] ring(float r, float a, int i, int j, int n, int m) {
		double u = 2 * Math.PI * i / n, v = 2 * Math.PI * j / m;
		float cu = (float) Math.cos(u), su = (float) Math.sin(u), cv = (float) Math.cos(v), sv = (float) Math.sin(v);
		return new float[] {(r + a * cv) * cu, a * sv, (r + a * cv) * su, cv * cu, sv, cv * su};
	}

	/** A flat disk of radius r in x-z with the accretion texture on it (the texture's own alpha makes it a ring). */
	private static void disk(PoseStack.Pose p, VertexConsumer vc, float r) {
		int n = 48;
		float inner = 0.6f * r;
		for (int i = 0; i < n; i++) {
			double a0 = 2 * Math.PI * i / n, a1 = 2 * Math.PI * (i + 1) / n;
			float c0 = (float) Math.cos(a0), s0 = (float) Math.sin(a0), c1 = (float) Math.cos(a1), s1 = (float) Math.sin(a1);
			float k = inner / r;
			// Outer and inner edge of this slice, with the texture mapped from its centre.
			float[][] pts = {
				{c0 * r, s0 * r, 0.5f + c0 * 0.5f, 0.5f + s0 * 0.5f},
				{c1 * r, s1 * r, 0.5f + c1 * 0.5f, 0.5f + s1 * 0.5f},
				{c1 * inner, s1 * inner, 0.5f + c1 * 0.5f * k, 0.5f + s1 * 0.5f * k},
				{c0 * inner, s0 * inner, 0.5f + c0 * 0.5f * k, 0.5f + s0 * 0.5f * k},
			};
			for (int face = 0; face < 2; face++) {
				for (int v = 0; v < 4; v++) {
					float[] q = pts[face == 0 ? v : 3 - v];
					vertex(p, vc, q[0], 0, q[1], q[2], q[3], FULL_BRIGHT, 0, face == 0 ? -1 : 1, 0);
				}
			}
		}
	}

	// ---------------------------------------------------------------- drawing

	private static void box(PoseStack.Pose p, VertexConsumer vc, int light, float x0, float y0, float z0, float x1, float y1, float z1, float[] uv) {
		float u0 = uv[0], v0 = uv[1], u1 = uv[2], v1 = uv[3];
		quad(p, vc, light, x0, y0, z0, x1, y0, z0, x1, y1, z0, x0, y1, z0, u0, v0, u1, v1, 0, 0, -1);
		quad(p, vc, light, x1, y0, z1, x0, y0, z1, x0, y1, z1, x1, y1, z1, u0, v0, u1, v1, 0, 0, 1);
		quad(p, vc, light, x0, y0, z1, x0, y0, z0, x0, y1, z0, x0, y1, z1, u0, v0, u1, v1, -1, 0, 0);
		quad(p, vc, light, x1, y0, z0, x1, y0, z1, x1, y1, z1, x1, y1, z0, u0, v0, u1, v1, 1, 0, 0);
		quad(p, vc, light, x0, y0, z1, x1, y0, z1, x1, y0, z0, x0, y0, z0, u0, v0, u1, v1, 0, -1, 0);
		quad(p, vc, light, x0, y1, z0, x1, y1, z0, x1, y1, z1, x0, y1, z1, u0, v0, u1, v1, 0, 1, 0);
	}

	/** Four corners in order (a, b, c, d), textured from (u0, v0) at a to (u1, v1) at c. */
	private static void quad(PoseStack.Pose p, VertexConsumer vc, int light,
			float ax, float ay, float az, float bx, float by, float bz, float cx, float cy, float cz, float dx, float dy, float dz,
			float u0, float v0, float u1, float v1, float nx, float ny, float nz) {
		vertex(p, vc, ax, ay, az, u0, v0, light, nx, ny, nz);
		vertex(p, vc, bx, by, bz, u1, v0, light, nx, ny, nz);
		vertex(p, vc, cx, cy, cz, u1, v1, light, nx, ny, nz);
		vertex(p, vc, dx, dy, dz, u0, v1, light, nx, ny, nz);
	}

	private static void vertex(PoseStack.Pose p, VertexConsumer vc, float x, float y, float z, float u, float v, int light, float nx, float ny, float nz) {
		vertex(p, vc, x, y, z, u, v, light, -1, nx, ny, nz);
	}

	private static void vertex(PoseStack.Pose p, VertexConsumer vc, float x, float y, float z, float u, float v, int light, int colour, float nx, float ny, float nz) {
		vc.addVertex(p, x, y, z).setColor(colour).setUv(u, v).setOverlay(OverlayTexture.NO_OVERLAY).setLight(light).setNormal(p, nx, ny, nz);
	}

	/** A quad seen from both sides. */
	private static void both(PoseStack.Pose p, VertexConsumer vc, int light,
			float ax, float ay, float az, float bx, float by, float bz, float cx, float cy, float cz, float dx, float dy, float dz,
			float u0, float v0, float u1, float v1, float nx, float ny, float nz) {
		quad(p, vc, light, ax, ay, az, bx, by, bz, cx, cy, cz, dx, dy, dz, u0, v0, u1, v1, nx, ny, nz);
		vertex(p, vc, ax, ay, az, u0, v0, light, -nx, -ny, -nz);
		vertex(p, vc, dx, dy, dz, u0, v1, light, -nx, -ny, -nz);
		vertex(p, vc, cx, cy, cz, u1, v1, light, -nx, -ny, -nz);
		vertex(p, vc, bx, by, bz, u1, v0, light, -nx, -ny, -nz);
	}
}
