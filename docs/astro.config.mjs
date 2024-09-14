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
        // {
        //   label: "Configuration",
        //   items
        // },
        // {
        //   label: "Guides",
        //   items: [
        //     // Each item here is one entry in the navigation menu.
        //     // { label: "Example Guide", slug: "guides/example" },
        //     { label: "Configuration", slug: "guides/config" },
        //   ],
        // },
        // {
        //   label: "Reference",
        //   autogenerate: { directory: "reference" },
        // },
      ],
    }),
  ],
});
