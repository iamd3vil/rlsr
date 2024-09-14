---
title: Release Targets 🎯
description: How to configure release targets in Rlsr.
---

The `targets` subsection in your Rlsr configuration specifies where the releases will be published. Rlsr supports multiple target types, allowing you to publish your releases to various platforms.

## Supported Targets

Currently, Rlsr supports the following targets:

1. GitHub
2. Docker

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

To release to GitHub, you must set the `GH_TOKEN` environment variable with a valid GitHub Personal Access Token. This token is used to authenticate and perform release operations on your GitHub repository.

## Docker

To publish Docker images, you need to provide the following information:

```yaml
targets:
  docker:
    image: "username/image:tag"
    dockerfile: "path/to/Dockerfile"
    context: "."
```

- `image`: The full name of the Docker image, including the registry if applicable.
- `dockerfile`: The path to the Dockerfile relative to your project root.
- `context`: The build context for Docker, usually the root of your project.

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
