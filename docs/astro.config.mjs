import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

// https://astro.build/config
export default defineConfig({
  integrations: [
    starlight({
      title: "Rlsr documentation",
      social: {
        github: "https://github.com/iamd3vil/rlsr",
      },
      sidebar: [
        "installation",
        {
          label: "Configuration",
          items: [
            { label: "Configuration", slug: "config/config" },
            { label: "Release Targets", slug: "config/targets" },
          ],
        },
      ],
    }),
  ],
});
