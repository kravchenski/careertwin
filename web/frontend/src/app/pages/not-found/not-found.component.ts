import { Component } from "@angular/core";
import { CommonModule } from "@angular/common";
import { RouterLink } from "@angular/router";

@Component({
  selector: "ct-not-found",
  standalone: true,
  imports: [CommonModule, RouterLink],
  template: `
    <div class="nf page-shell">
      <div class="nf__inner">
        <div class="nf__code">404</div>
        <h1 class="nf__title">This page took a different career path.</h1>
        <p class="nf__copy">
          The link you followed doesn't exist or the page moved. Let's get you back to
          something useful.
        </p>
        <div class="nf__actions">
          <a routerLink="/" class="btn-primary">Back to home</a>
          <a routerLink="/studio" class="btn-secondary">Open Studio</a>
        </div>
        <div class="nf__orb" aria-hidden="true"></div>
      </div>
    </div>
  `,
  styles: [`
    .nf {
      min-height: 72dvh;
      display: grid;
      place-items: center;
      padding: 4rem 0;
    }
    .nf__inner {
      position: relative;
      text-align: center;
      max-width: 42rem;
    }
    .nf__code {
      font-family: var(--font-display);
      font-size: clamp(6rem, 18vw, 12rem);
      line-height: 0.82;
      letter-spacing: 0.02em;
      color: transparent;
      -webkit-text-stroke: 2px rgba(209, 161, 87, 0.45);
      user-select: none;
    }
    .nf__title {
      font-family: var(--font-display);
      font-size: clamp(1.8rem, 5vw, 3rem);
      line-height: 0.95;
      letter-spacing: 0.01em;
      text-transform: uppercase;
      color: var(--color-foreground);
      margin: 0.5rem 0 0.8rem;
    }
    .nf__copy {
      color: var(--color-muted);
      line-height: 1.7;
      margin: 0 auto 1.6rem;
      max-width: 34rem;
    }
    .nf__actions {
      display: flex;
      gap: 0.75rem;
      justify-content: center;
      flex-wrap: wrap;
    }
    .nf__orb {
      position: absolute;
      top: -3rem;
      left: 50%;
      transform: translateX(-50%);
      width: min(20rem, 60vw);
      aspect-ratio: 1;
      border-radius: 999px;
      background: radial-gradient(circle at 40% 35%, rgba(245, 223, 144, 0.22), rgba(209, 161, 87, 0.1) 55%, transparent 70%);
      filter: blur(8px);
      z-index: -1;
      pointer-events: none;
    }
  `],
})
export class NotFoundComponent {}
