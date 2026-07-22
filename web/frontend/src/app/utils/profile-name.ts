const PLACEHOLDER_NAMES = new Set([
  "unknown",
  "linkedin user",
  "candidate",
  "applicant",
  "your name",
  "your twin",
  "professional",
]);

export function isRealProfileName(value: unknown): value is string {
  if (typeof value !== "string") return false;
  const normalized = value.trim().replace(/\s+/g, " ").toLowerCase();
  return normalized.length >= 2 && !PLACEHOLDER_NAMES.has(normalized);
}

export function resolveProfileName(
  profileName: unknown,
  firstName: unknown = "",
  lastName: unknown = "",
): string {
  if (isRealProfileName(profileName)) {
    return profileName.trim().replace(/\s+/g, " ");
  }
  const composed = [firstName, lastName]
    .filter((part): part is string => typeof part === "string" && part.trim().length > 0)
    .map((part) => part.trim())
    .join(" ");
  return isRealProfileName(composed) ? composed : "";
}
