<!-- SPDX-License-Identifier: MIT -->
<script lang="ts">
  import {t} from '../lib/i18n';
  import type {WorldAnchor} from '../lib/world-state';
  let {anchors}: {anchors:readonly WorldAnchor[]}=$props();
</script>

<!-- Exactly one caller-owned layer per output surface. This renders public
     anchors only and does not reserve a slot or acknowledge an action. -->
<div class="world-object-layer" aria-hidden="true">
  {#each anchors as anchor (anchor.id)}
    <span class="world-object" style:left={`${anchor.position.x/10000}%`} style:top={`${anchor.position.y/10000}%`}
      title={$t(anchor.kind==='rest'?'Ruhe- oder Ladeplatz':anchor.kind==='meeting'?'Treffpunkt':'Spielobjekt')}>
      {anchor.kind==='rest'?'◒':anchor.kind==='meeting'?'◎':'◇'}
    </span>
  {/each}
</div>

<style>
  .world-object-layer{position:absolute;inset:0;pointer-events:none;z-index:20}
  .world-object{position:absolute;transform:translate(-50%,-50%);color:var(--accent,#9faeea);font-size:24px;line-height:1}
</style>
