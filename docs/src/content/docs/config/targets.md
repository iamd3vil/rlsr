---
title: Release Targets 🎯
description: How to configure release targets in Rlsr.
---

The `targets` subsection in your Rlsr configuration specifies where the releases will be published. Rlsr supports multiple target types, allowing you to publish your releases to various platforms.

## Supported Targets

Currently, Rlsr supports the following targets:

1. GitHub
2. GitLab
3. Docker

Let's explore each target type in detail.

## GitHub

To release to GitHub, you need to specify the following in your configuration:

```yaml
targets:
  github:
    owner: "username"
    repo: "repository"
```

- `owner`: The GitHub username or organization name where the repository is hosted.
- `repo`: The name of the GitHub repository.

### Important Note

To release to GitHub, you must set the `GITHUB_TOKEN` environment variable with a valid GitHub Personal Access Token. This token is used to authenticate and perform release operations on your GitHub repository.

## GitLab

To release to GitLab, you need to specify the following in your configuration:

```yaml
targets:
  gitlab:
    owner: "namespace"
    repo: "project-name"
    url: "https://gitlab.com" # optional, for self-hosted instances
```

- `owner`: The GitLab group/namespace or username where the project lives.
- `repo`: The GitLab project name.
- `url`: The GitLab instance URL (optional, defaults to `https://gitlab.com`).

### Important Note

To release to GitLab, you must set the `GITLAB_TOKEN` environment variable with a valid GitLab Personal Access Token that has the **api** scope.

For more details on GitLab behavior, see [GitLab Releases](/config/gitlab).

## Docker

To publish Docker images, you can either build + push an image or push existing tags.

### Build and push (legacy)

```yaml
targets:
  docker:
    image: "username/image:tag"
    dockerfile: "path/to/Dockerfile"
    context: "."
    push: true
```

- `image`: The full name of the Docker image, including the registry if applicable.
- `dockerfile`: The path to the Dockerfile relative to your project root.
- `context`: The build context for Docker, usually the root of your project.
- `push`: Optional. Defaults to `true`. Set to `false` to skip `docker push`.

The `image` value supports templating (see [Templating](/templating/)). If you omit a tag or digest, rlsr appends the current tag automatically.

### Push existing images

```yaml
targets:
  docker:
    images:
      - "ghcr.io/acme/app:{{ meta.tag }}"
      - "ghcr.io/acme/app:latest"
    push: true
```

- `images`: Optional list of image references to push. Each entry supports templating and will receive the latest tag if one is missing.

### Buildx publishing

If you build images with `type: "buildx"`, rlsr records the rendered Buildx tags during the build. When a Docker target is configured without `image`/`dockerfile`/`context` or `images`, rlsr pushes the Buildx tags instead. This lets Buildx builds publish images without a separate docker build step.

When using Buildx outputs:
- `type=registry` pushes the multi-arch image during the build step. In this case, set `targets.docker.push: false` to avoid a second push pass.
- `load: true` loads a single-platform image into the local Docker daemon; the Docker target can push it when `targets.docker.push` is true, but it is not multi-arch.
- `type=local` or `type=tar` writes files locally and does not publish.

`targets.docker.push` only controls whether the Docker target performs `docker push` for images it handles (configured `image`/`images` or captured Buildx tags). It does not change the Buildx build behavior.

## Multiple Targets

You can specify multiple targets in your configuration to release to different platforms simultaneously. For example:

```yaml
targets:
  github:
    owner: "username"
    repo: "repository"
  docker:
    image: "username/image:tag"
    dockerfile: "./Dockerfile"
    context: "."
```

This configuration will release your project to both GitHub and Docker.
