export function audioBlobToBase64(blob: Blob) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new Error("Kunne ikke læse lydfilen."));
    reader.onloadend = () => {
      const result = String(reader.result);
      resolve(result.split(",")[1] ?? "");
    };
    reader.readAsDataURL(blob);
  });
}

export async function acquireUserAudio() {
  if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) {
    throw new Error("Din app kan ikke få adgang til mikrofon-API'et. Genstart appen, eller opdater Tauri.");
  }

  const candidates: MediaStreamConstraints[] = [
    { audio: true },
    { audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true } as MediaTrackConstraints },
    { audio: { echoCancellation: false, noiseSuppression: false, autoGainControl: false } as MediaTrackConstraints },
  ];

  let lastError: unknown;
  for (const constraints of candidates) {
    try {
      return await navigator.mediaDevices.getUserMedia(constraints);
    } catch (error) {
      lastError = error;
    }
  }

  throw lastError instanceof Error ? lastError : new Error(String(lastError));
}

export function describeMicError(caught: unknown) {
  if (caught instanceof Error) {
    const m = caught.message.toLowerCase();
    if (m.includes("not allowed") || m.includes("denied") || m.includes("permission") || m.includes("securityerror")) {
      return "Mikrofon blev afvist. Gå til Systemindstillinger → Fortrolighed og sikkerhed → Mikrofon, og slå Hey Mikkel til (eller kør igen efter en genstart af appen).";
    }
  }

  if (caught instanceof DOMException) {
    if (caught.name === "NotAllowedError" || caught.name === "SecurityError") {
      return "Mikrofon blev afvist. Slå Mikrofon til for Hey Mikkel i Systemindstillinger → Fortrolighed og sikkerhed → Mikrofon.";
    }

    if (caught.name === "NotFoundError" || caught.name === "OverconstrainedError") {
      return "Fandt ingen mikrofon, eller lydkortet svarer ikke. Tjek lydindstillinger og vælg en input-enhed i macOS Lydindstillinger.";
    }

    if (caught.name === "NotReadableError") {
      return "Mikrofonen er i brug af en anden app, eller lydkortet er låst. Luk andre optagere og prøv igen.";
    }

    if (caught.message.includes("Invalid constraint")) {
      return "Kunne ikke starte mikrofonen. Genstart appen, og giv den Mikrofon-tilladelse. Hvis det fortsætter, tjek lydenheds-input i Systemindstillinger.";
    }

    return caught.message;
  }

  return "Kunne ikke starte mikrofonen. Tjek at Hey Mikkel har Mikrofontilladelse under Systemindstillinger → Fortrolighed og sikkerhed → Mikrofon, og at ingen anden app bruger inputtet i forvejen.";
}
