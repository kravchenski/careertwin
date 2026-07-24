import { Component } from "@angular/core";
import { CommonModule } from "@angular/common";

@Component({
  selector: "ct-cookie-banner",
  standalone: true,
  imports: [CommonModule],
  template: `
    <div class="cookie-banner" *ngIf="!dismissed">
      <div class="cookie-banner__inner">
        <p>
          We use cookies to improve your experience. By continuing to use JobTwin, you agree to our
          <a href="https://jobtwin.app/privacy" target="_blank">Privacy Policy</a>.
        </p>
        <div class="cookie-banner__actions">
          <button class="cookie-btn cookie-btn--accept" (click)="accept()">Accept</button>
          <button class="cookie-btn cookie-btn--decline" (click)="dismiss()">Decline</button>
        </div>
      </div>
    </div>
  `,
  styles: [`
    .cookie-banner {
      position: fixed;
      bottom: 0;
      left: 0;
      right: 0;
      z-index: 1000;
      padding: 1rem;
      background: rgba(18, 11, 7, 0.95);
      backdrop-filter: blur(20px);
      border-top: 1px solid rgba(244, 229, 205, 0.1);
    }

    .cookie-banner__inner {
      max-width: 80rem;
      margin: 0 auto;
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 1.5rem;
    }

    .cookie-banner p {
      color: var(--color-muted, #baa690);
      font-size: 0.875rem;
      line-height: 1.5;
      flex: 1;
    }

    .cookie-banner a {
      color: var(--color-foreground, #f4e5cd);
      text-decoration: underline;
      text-underline-offset: 2px;
    }

    .cookie-banner__actions {
      display: flex;
      gap: 0.75rem;
      flex-shrink: 0;
    }

    .cookie-btn {
      padding: 0.5rem 1.25rem;
      border-radius: 999px;
      font-size: 0.8125rem;
      font-weight: 600;
      cursor: pointer;
      transition: all 0.2s;
      border: none;
    }

    .cookie-btn--accept {
      background: #c89b50;
      color: #0f0b08;
    }

    .cookie-btn--accept:hover {
      background: #d4a85c;
    }

    .cookie-btn--decline {
      background: transparent;
      border: 1px solid rgba(200, 155, 80, 0.2);
      color: var(--color-muted, #a89880);
    }

    .cookie-btn--decline:hover {
      border-color: rgba(244, 229, 205, 0.4);
      color: var(--color-foreground, #f4e5cd);
    }

    @media (max-width: 640px) {
      .cookie-banner__inner {
        flex-direction: column;
        text-align: center;
      }

      .cookie-banner__actions {
        width: 100%;
        justify-content: center;
      }
    }
  `],
})
export class CookieBannerComponent {
  dismissed = false;

  constructor() {
    const consent = localStorage.getItem("cookie-consent");
    this.dismissed = consent === "accepted" || consent === "declined";
  }

  accept() {
    localStorage.setItem("cookie-consent", "accepted");
    this.dismissed = true;
  }

  dismiss() {
    localStorage.setItem("cookie-consent", "declined");
    this.dismissed = true;
  }
}
