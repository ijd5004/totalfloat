import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { Api, takeSession } from "./api";
import { Registry } from "./commands";
import { Shell } from "./shell/Shell";
import "./styles.css";

const session = takeSession(window.location, window.localStorage, () => {
  window.history.replaceState(null, "", window.location.pathname + window.location.search);
});

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <Shell api={new Api(session)} registry={new Registry()} />
    </StrictMode>,
  );
}
