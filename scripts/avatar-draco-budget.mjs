// Serialized verbatim into the reviewed Draco wrapper; keep self-contained.
export function installDracoOutputBudget(module, maximum = 256 * 1024 * 1024, inputBudgets = new WeakMap()) {
  const geometryBudgets = new WeakMap();
  // Missing trusted task tokens share one conservative generation-wide budget.
  const fallback = { used: 0 };
  function reserve(geometry, bytes) {
    const budget = geometryBudgets.get(geometry) || fallback;
    const total = budget.used + bytes;
    if (!Number.isSafeInteger(bytes) || bytes < 0 || !Number.isSafeInteger(total) || total > maximum) throw new Error('Die komprimierten Modelldaten sind ungültig oder überschreiten das Ressourcenlimit.');
    budget.used = total;
  }
  const prototype = module.Decoder.prototype;
  for (const name of ['DecodeArrayToMesh', 'DecodeArrayToPointCloud']) {
    const decode = prototype[name];
    prototype[name] = function (array, length, geometry) {
      geometryBudgets.set(geometry, inputBudgets.get(array.buffer) || fallback);
      return decode.call(this, array, length, geometry);
    };
  }
  const attribute = prototype.GetAttributeDataArrayForAllPoints;
  prototype.GetAttributeDataArrayForAllPoints = function (geometry, source, type, bytes, pointer) {
    const count = geometry.num_points();
    const stride = bytes / count;
    if (!Number.isSafeInteger(count) || count <= 0 || !Number.isSafeInteger(stride) || stride <= 0) throw new Error('Ungültige Draco-Attributgröße.');
    // Three has already called wasm_malloc (separately capped linear memory).
    // Account for its padded JS output before copying/allocating that output.
    reserve(geometry, count * Math.ceil(stride / 4) * 4);
    return attribute.call(this, geometry, source, type, bytes, pointer);
  };
  const indices = prototype.GetTrianglesUInt32Array;
  prototype.GetTrianglesUInt32Array = function (geometry, bytes, pointer) {
    reserve(geometry, bytes);
    return indices.call(this, geometry, bytes, pointer);
  };
}
