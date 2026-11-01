export const cx = (...xs: Array<string | false | null | undefined>) => xs.filter(Boolean).join(" ");
