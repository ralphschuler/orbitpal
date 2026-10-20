<script lang="ts">
  import { t } from '../lib/i18n';
  import HelpTip from './HelpTip.svelte';
</script>

<p class="field-note credential-help">
  <span>{$t("Gespeicherte Schlüssel bleiben im Systemschlüsselbund und werden nicht in Profile exportiert.")}</span>
  <HelpTip label={$t("Schlüsselbundzugriff erklären")} text={$t("Unter macOS ist das Speichern eines Schlüssels von der Zugriffsfreigabe getrennt. „Erlauben“ gilt nur für den aktuellen Abruf. „Immer erlauben“ merkt sich diese App für diesen Schlüssel. Verschiedene Schlüssel können getrennte Freigaben benötigen. Ein gesperrter Schlüsselbund kann erneut nach seinem Passwort fragen. Mikrofon- und Bildschirmrechte werden separat verwaltet.")}/>
</p>

<style>
  .credential-help { display: flex; align-items: flex-start; gap: 4px; }
</style>
