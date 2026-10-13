import { AfterViewInit, Component, ElementRef, ViewChild, inject } from "@angular/core";
import { CommonModule } from "@angular/common";
import { Router, RouterLink, RouterOutlet, NavigationEnd } from "@angular/router";
import { filter } from "rxjs/operators";
import { CookieBannerComponent } from "./components/cookie-banner/cookie-banner.component";
import { ProfileComponent } from "./pages/profile/profile.component";
import { ProfileService } from "./services/profile.service";
import { StudioService } from "./services/studio.service";
import { resolveProfileName } from "./utils/profile-name";
import gsap from "gsap";

@Component({
  selector: "ct-root",
  standalone: true,
  imports: [CommonModule, RouterOutlet, RouterLink, CookieBannerComponent, ProfileComponent],
  template: `
    <div class="min-h-screen">
      <a class="skip-link" href="#main-content">Skip to content</a>
      <header #header class="sticky top-0 z-50 px-3 pt-3 sm:px-0">
        <div class="page-shell app-topbar flex items-center justify-between rounded-[2rem] border border-[var(--color-line)] bg-[var(--color-panel)] px-4 py-3">
          <a routerLink="/" class="inline-flex items-center gap-3 no-underline">
            <span class="inline-flex h-10 w-10 items-center justify-center rounded-full border border-[var(--color-line-strong)] bg-[var(--color-panel-strong)] text-[var(--color-foreground)]">
              <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6">
                <path d="M4 6.5C4 5.12 5.12 4 6.5 4H12a8 8 0 1 1 0 16H6.5A2.5 2.5 0 0 1 4 17.5z"/>
                <path d="M8 9h6M8 12h8M8 15h5"/>
              </svg>
            </span>
            <strong class="text-lg font-semibold tracking-[-0.02em] text-[var(--color-foreground)]" style="font-family: 'Space Grotesk', sans-serif;">JobTwin</strong>
          </a>

          <div class="flex items-center gap-3">
            <a routerLink="/agents" class="btn-secondary">Agents</a>
            <a routerLink="/studio" class="btn-primary" *ngIf="!hasProfile() && !isStudio()">
              <span>Get Started</span>
              <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M5 12h14M13 5l7 7-7 7"/>
              </svg>
            </a>

            <div class="account" *ngIf="hasProfile()">
              <button
                class="profile-avatar"
                type="button"
                (click)="accountOpen = !accountOpen"
                [attr.aria-expanded]="accountOpen"
                aria-label="Account menu"
              >
                {{ profileInitials() }}
              </button>

              <div class="account-backdrop" *ngIf="accountOpen" (click)="accountOpen = false"></div>
              <div class="account-menu" *ngIf="accountOpen">
                <div class="account-menu__head">
                  <div class="account-menu__name">{{ profileName() }}</div>
                  <div class="account-menu__email">{{ profileEmail() }}</div>
                </div>
                <button class="account-item" type="button" (click)="openDashboard()">
                  <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.7">
                    <rect x="3" y="3" width="7" height="9" rx="1.5"/><rect x="14" y="3" width="7" height="5" rx="1.5"/><rect x="14" y="12" width="7" height="9" rx="1.5"/><rect x="3" y="16" width="7" height="5" rx="1.5"/>
                  </svg>
                  Dashboard
                </button>
                <button class="account-item" type="button" (click)="openProfile()">
                  <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.7">
                    <circle cx="12" cy="8" r="4"/><path d="M4 20c0-4 3.6-7 8-7s8 3 8 7"/>
                  </svg>
                  Profile
                </button>
                <button class="account-item account-item--danger" type="button" (click)="resetStudio()">
                  <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.7">
                    <path d="M3 6h18M8 6V4h8v2m-9 0v14h10V6"/>
                  </svg>
                  Reset Studio
                </button>
              </div>
            </div>
          </div>
        </div>
      </header>

      <main id="main-content" class="px-0 py-6 sm:py-8 pb-20">
        <router-outlet></router-outlet>
      </main>

      <div class="profile-drawer-backdrop" *ngIf="profileOpen" (click)="profileOpen = false"></div>
      <aside class="profile-drawer" *ngIf="profileOpen">
        <div class="profile-drawer__head">
          <span class="eyebrow">Your Profile</span>
          <button class="drawer-close" type="button" (click)="profileOpen = false" aria-label="Close">×</button>
        </div>
        <div class="profile-drawer__body">
          <ct-profile />
        </div>
      </aside>

      <ct-cookie-banner />
    </div>
  `,
  styles: [`
    @media (max-width: 640px) {
      .app-topbar { flex-wrap: wrap; gap: 0.75rem; }
      .app-topbar > .flex { flex-wrap: wrap; }
    }

    :host ::ng-deep nav a.active {
      color: var(--color-foreground);
    }

    .skip-link {
      position: absolute;
      left: 0.5rem;
      top: -3rem;
      z-index: 200;
      padding: 0.6rem 1rem;
      border-radius: 0.8rem;
      background: #ecd4ab;
      color: #120b07;
      font-weight: 600;
      text-decoration: none;
      transition: top 160ms ease;
    }
    .skip-link:focus {
      top: 0.5rem;
    }

    .profile-avatar {
      width: 2.6rem; height: 2.6rem; border-radius: 50%; flex-shrink: 0;
      display: grid; place-items: center; cursor: pointer;
      font-family: var(--font-display); font-size: 0.95rem; color: #120b07;
      background: linear-gradient(135deg, #f5df90, #d1a157);
      border: 1px solid rgba(255,255,255,0.25); transition: transform 160ms;
    }
    .profile-avatar:hover { transform: scale(1.06); }

    .account { position: relative; }
    .account-backdrop { position: fixed; inset: 0; z-index: 90; }
    .account-menu {
      position: absolute; top: calc(100% + 0.6rem); right: 0; z-index: 95;
      width: 15rem; border: 1px solid var(--color-line); border-radius: 1.2rem;
      background: #15100a; box-shadow: 0 24px 60px rgba(10,6,3,0.5); padding: 0.6rem;
      animation: menuIn 200ms cubic-bezier(0.22,1,0.36,1);
    }
    @keyframes menuIn { from { opacity: 0; transform: translateY(-8px); } to { opacity: 1; transform: none; } }
    .account-menu__head { padding: 0.6rem 0.7rem 0.8rem; border-bottom: 1px solid var(--color-line); margin-bottom: 0.5rem; }
    .account-menu__name { font-weight: 600; font-size: 0.95rem; }
    .account-menu__email { font-size: 0.78rem; color: var(--color-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .account-item {
      display: flex; align-items: center; gap: 0.6rem; width: 100%; text-align: left;
      border: none; background: transparent; color: var(--color-foreground);
      padding: 0.65rem 0.7rem; border-radius: 0.8rem; cursor: pointer; font-size: 0.9rem; transition: background 150ms;
    }
    .account-item:hover { background: rgba(255,255,255,0.06); }
    .account-item--danger { color: #f0a09a; }
    .account-item--danger:hover { background: rgba(240,160,154,0.1); }

    .profile-drawer-backdrop {
      position: fixed; inset: 0; z-index: 90;
      background: rgba(8,5,3,0.6); backdrop-filter: blur(4px);
    }
    .profile-drawer {
      position: fixed; top: 0; right: 0; bottom: 0; z-index: 95;
      width: min(40rem, 100vw); overflow-y: auto;
      background: #0d0a07; border-left: 1px solid var(--color-line);
      box-shadow: -30px 0 80px rgba(10,6,3,0.5);
      padding: 1.2rem; animation: drawerIn 320ms cubic-bezier(0.22,1,0.36,1);
    }
    @keyframes drawerIn { from { transform: translateX(100%); } to { transform: none; } }
    .profile-drawer__head {
      display: flex; align-items: center; justify-content: space-between;
      margin-bottom: 1rem; position: sticky; top: 0; background: #0d0a07; padding: 0.3rem 0; z-index: 2;
    }
    .drawer-close {
      width: 2.2rem; height: 2.2rem; border-radius: 50%; border: 1px solid var(--color-line);
      background: transparent; color: var(--color-foreground); font-size: 1.3rem; cursor: pointer;
    }
    .drawer-close:hover { background: rgba(255,255,255,0.06); }
    .profile-drawer__body { min-height: 60vh; }

    @media (max-width: 640px) {
      .profile-avatar { width: 2.3rem; height: 2.3rem; font-size: 0.85rem; }
      .account-menu { width: 13rem; }
    }
  `],
})
export class AppComponent implements AfterViewInit {
  @ViewChild("header") headerRef?: ElementRef<HTMLElement>;
  @ViewChild(ProfileComponent) profileRef?: ProfileComponent;
  profileOpen = false;
  accountOpen = false;

  private readonly profileService = inject(ProfileService);
  private readonly studio = inject(StudioService);
  private readonly router = inject(Router);
  currentUrl = "";

  constructor() {
    this.router.events
      .pipe(filter((e): e is NavigationEnd => e instanceof NavigationEnd))
      .subscribe((e) => (this.currentUrl = e.urlAfterRedirects));
  }

  hasProfile(): boolean {
    return !!this.studio.load()?.completedAt;
  }

  isStudio(): boolean {
    return this.currentUrl.startsWith("/studio");
  }

  profileInitials(): string {
    const studioState = this.studio.load();
    const p = studioState?.profile ?? this.profileService.load();
    const n = resolveProfileName(
      p.fullName,
      studioState?.firstName,
      studioState?.lastName,
    );
    if (!n) return "CT";
    const parts = n.split(/\s+/);
    return (parts[0][0] + (parts[1]?.[0] ?? "")).toUpperCase();
  }
  profileName(): string {
    const s = this.studio.load();
    return resolveProfileName(
      s?.profile.fullName || this.profileService.load().fullName,
      s?.firstName,
      s?.lastName,
    ) || "Your Twin";
  }
  profileEmail(): string {
    const s = this.studio.load();
    return s?.preferences.contactEmail || s?.profile.email || this.profileService.load().email || "";
  }

  openDashboard(): void {
    this.accountOpen = false;
    this.router.navigate(["/dashboard"]);
  }

  openProfile(): void {
    this.accountOpen = false;
    this.profileOpen = true;
  }

  resetStudio(): void {
    this.accountOpen = false;
    this.studio.clear();
    this.profileService.clear();
    this.studio.deleteBackend().subscribe(() => {
      this.profileService.deleteBackend().subscribe(() => {
        this.router.navigate(["/studio"]);
      });
    });
  }

  ngAfterViewInit() {
    if (this.headerRef) {
      gsap.from(this.headerRef.nativeElement, {
        y: -60,
        opacity: 0,
        duration: 0.8,
        ease: "power3.out",
        delay: 0.2,
      });
    }
  }
}
