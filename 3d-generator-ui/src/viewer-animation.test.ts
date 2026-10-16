import assert from "node:assert/strict";
import test from "node:test";
import * as THREE from "three";
import { ModelAnimation } from "./viewer-animation.ts";

function fixture() {
  const root = new THREE.Group();
  const bone = new THREE.Bone();
  bone.name = "joint";
  root.add(bone);
  const clip = new THREE.AnimationClip("Move", 1, [
    new THREE.VectorKeyframeTrack("joint.position", [0, 1], [0, 0, 0, 1, 0, 0]),
  ]);
  return { bone, playback: new ModelAnimation(root, [clip]) };
}

test("animation loads paused and seeks without starting playback", () => {
  const { bone, playback } = fixture();
  assert.equal(playback.state().playing, false);
  playback.seek(0.5);
  assert.ok(Math.abs(bone.position.x - 0.5) < 0.001);
  assert.equal(playback.update(0.1), false);
  playback.dispose();
});

test("playback advances at the chosen speed and bounds background time jumps", () => {
  const { playback } = fixture();
  playback.setSpeed(2);
  playback.setPlaying(true);
  playback.update(60);
  assert.ok(Math.abs(playback.state().time - 0.2) < 0.001);
  playback.setPlaying(false);
  playback.update(0.1);
  assert.ok(Math.abs(playback.state().time - 0.2) < 0.001);
  playback.dispose();
});

test("reset restores authored rest pose and invalid selections cannot lose it", () => {
  const { bone, playback } = fixture();
  playback.seek(0.7);
  playback.resetPose();
  assert.equal(bone.position.x, 0);
  assert.equal(playback.state().selected, -1);
  playback.select(99);
  assert.equal(playback.state().selected, -1);
  playback.select(0);
  assert.equal(playback.state().selected, 0);
  playback.dispose();
  assert.deepEqual(playback.state().clips, []);
});

test("loop off clamps at the clip end and stops playback; loop on wraps", () => {
  const looped = fixture();
  assert.equal(looped.playback.state().loop, true);
  looped.playback.setPlaying(true);
  for (let tick = 0; tick < 12; tick += 1) looped.playback.update(0.1);
  assert.ok(Math.abs(looped.playback.state().time - 0.2) < 0.001);
  assert.equal(looped.playback.state().playing, true);
  looped.playback.dispose();

  const once = fixture();
  once.playback.setLoop(false);
  assert.equal(once.playback.state().loop, false);
  once.playback.setPlaying(true);
  for (let tick = 0; tick < 6; tick += 1) once.playback.update(0.1);
  assert.ok(Math.abs(once.playback.state().time - 0.6) < 0.001);
  assert.equal(once.playback.state().playing, true);
  for (let tick = 0; tick < 6; tick += 1) once.playback.update(0.1);
  assert.ok(Math.abs(once.playback.state().time - 1) < 0.001);
  assert.equal(once.playback.state().playing, false);
  assert.equal(once.playback.update(0.1), false);
  assert.ok(Math.abs(once.playback.state().time - 1) < 0.001);
  once.playback.dispose();
});
