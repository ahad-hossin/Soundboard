export function validatePort(value: string): string | null {
  if (!/^\d+$/.test(value)) return "Port must be a number";
  const n = Number(value);
  if (n < 1024 || n > 65535) return "Port must be between 1024 and 65535";
  return null;
}

export function validatePin(value: string): string | null {
  return /^\d{4,8}$/.test(value) ? null : "PIN must be 4 to 8 digits";
}
