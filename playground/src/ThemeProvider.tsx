import { PaletteOptions, ThemeProvider as MuiThemeProvider, createTheme } from "@mui/material";
import { useLayoutEffect, useMemo, useState } from "react";

declare module "@mui/material/styles" {
  interface TypographyVariants {
    fontMonospace: string;
  }
  interface TypographyVariantsOptions {
    fontMonospace: string;
  }
}

export const fontSansSerif = "'Inter'";
export const fontMonospace = "'IBM Plex Mono'";

export const darkPalette: PaletteOptions = {
  mode: "dark",
  tonalOffset: 0.15,
  primary: { main: "#9382FE" },
  secondary: { main: "#b1b1b1" },
  error: { main: "#f54966" },
  warning: { main: "#eba800" },
  success: { main: "#92c353" },
  info: { main: "#29bee7" },
  text: {
    primary: "#e1e1e4",
    secondary: "#a7a6af",
    disabled: "rgba(255, 255, 255, 0.55)",
  },
  divider: "#585861",
  background: {
    default: "#121212",
    paper: "#29292c",
  },
  grey: {
    50: "#45474d",
    100: "#3b3b44",
    200: "#35353d",
    300: "#33333a",
    400: "#2f2f35",
    500: "#2d2d33",
    600: "#27272b",
    700: "#212127",
    800: "#16161b",
    900: "#121217",
    A100: "#d2d5df",
    A200: "#aeb0b7",
    A400: "#60636c",
    A700: "#313138",
  },
};

export const lightPalette: PaletteOptions = {
  mode: "light",
  tonalOffset: 0.22,
  primary: { main: "#5933F2" },
  secondary: { main: "#808080" },
  error: { main: "#db3553" },
  warning: { main: "#eba800" },
  success: { main: "#107c10" },
  info: { main: "#1EA7FD" },
  background: {
    default: "#ffffff",
    paper: "#ffffff",
  },
  text: {
    primary: "#393939",
    secondary: "#6f6d79",
    disabled: "rgba(0, 0, 0, 0.5)",
  },
  divider: "#D6d6d6",
  grey: {
    50: "#fafafa",
    100: "#f5f5f5",
    200: "#eeeeee",
    300: "#e0e0e0",
    400: "#bdbdbd",
    500: "#9e9e9e",
    600: "#757575",
    700: "#616161",
    800: "#424242",
    900: "#212121",
    A100: "#d5d5d5",
    A200: "#aaaaaa",
    A400: "#616161",
    A700: "#303030",
  },
};

export function ThemeProvider(props: React.PropsWithChildren): React.JSX.Element {
  const [isDark, setIsDark] = useState(() => matchMedia("(prefers-color-scheme: dark)").matches);
  useLayoutEffect(() => {
    function listener(this: MediaQueryList) {
      setIsDark(this.matches);
    }
    const query = matchMedia("(prefers-color-scheme: dark)");
    query.addEventListener("change", listener);
    return () => {
      query.removeEventListener("change", listener);
    };
  }, []);

  const theme = useMemo(
    () =>
      createTheme({
        palette: isDark ? darkPalette : lightPalette,
        typography: {
          fontMonospace,
          fontFamily: fontSansSerif,
          fontSize: 12,
          button: {
            textTransform: "none",
          },
        },
        shape: {
          borderRadius: 2,
        },
        transitions: {
          // So `transition: none;` gets applied everywhere
          create: () => "none",
        },
        components: {
          MuiButtonBase: {
            defaultProps: {
              disableRipple: true,
            },
          },
          MuiButton: {
            defaultProps: {
              disableElevation: true,
            },
          },
        },
      }),
    [isDark],
  );
  return <MuiThemeProvider theme={theme}>{props.children}</MuiThemeProvider>;
}
