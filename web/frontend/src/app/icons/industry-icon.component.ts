import { Component, Input } from "@angular/core";
import { CommonModule } from "@angular/common";

@Component({
  selector: "ct-industry-icon",
  standalone: true,
  imports: [CommonModule],
  template: `
    <ng-container [ngSwitch]="name">
      <svg *ngSwitchCase="'it'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="12" rx="1.5"/><path d="M2 20h20"/></svg>
      <svg *ngSwitchCase="'healthcare'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><path d="M7 12h3l2-4 2 8 2-4h3"/></svg>
      <svg *ngSwitchCase="'finance'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M3 10l9-6 9 6"/><path d="M4 10v9M9 10v9M15 10v9M20 10v9"/><path d="M2 21h20"/></svg>
      <svg *ngSwitchCase="'sales'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M3 17l6-6 4 4 8-8"/><path d="M15 7h6v6"/></svg>
      <svg *ngSwitchCase="'marketing'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M3 10v4h3l6 4V6l-6 4H3z"/><path d="M17 9a4 4 0 0 1 0 6"/></svg>
      <svg *ngSwitchCase="'retail'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M6 8V6a6 6 0 0 1 12 0v2"/><rect x="3" y="8" width="18" height="13" rx="2"/></svg>
      <svg *ngSwitchCase="'education'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M2 9l10-5 10 5-10 5-10-5z"/><path d="M6 11.5v4c0 1.4 2.7 2.5 6 2.5s6-1.1 6-2.5v-4"/><path d="M22 9v6"/></svg>
      <svg *ngSwitchCase="'hr'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><circle cx="8.5" cy="8" r="3"/><circle cx="16.5" cy="9" r="2.5"/><path d="M2.5 20c0-3.3 2.7-6 6-6s6 2.7 6 6"/><path d="M15 14.5c2.5.3 4.5 2.5 4.5 5.5"/></svg>
      <svg *ngSwitchCase="'hospitality'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M3 19v-7a2 2 0 0 1 2-2h5a2 2 0 0 1 2 2v3"/><path d="M2 14h20v5"/><path d="M21 19v-2a2 2 0 0 0-2-2h-3"/><circle cx="7" cy="10.5" r="1.4"/><path d="M2 19v2M22 19v2"/></svg>
      <svg *ngSwitchCase="'engineering'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M14.7 6.3a4 4 0 0 0-5.4 5.4L3 18l3 3 6.3-6.3a4 4 0 0 0 5.4-5.4l-2.8 2.8-2.6-2.6z"/></svg>
      <svg *ngSwitchCase="'legal'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3v18"/><path d="M8 21h8"/><path d="M5 7h14"/><path d="M5 7l-3 6a3 3 0 0 0 6 0z"/><path d="M19 7l-3 6a3 3 0 0 0 6 0z"/></svg>
      <svg *ngSwitchCase="'media'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><rect x="2.5" y="7" width="19" height="13" rx="2"/><path d="M8 7l1.6-2.5h4.8L16 7"/><circle cx="12" cy="13.5" r="3.5"/></svg>
      <svg *ngSwitchCase="'government'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M4 21h16"/><path d="M5 21V10.5l7-4.5 7 4.5V21"/><path d="M9.5 21v-5.5h5V21"/></svg>
      <svg *ngSwitchCase="'nonprofit'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20s-7.2-4.5-9.6-9.1A5 5 0 0 1 12 5.8a5 5 0 0 1 9.6 5.1C19.2 15.5 12 20 12 20z"/></svg>
      <svg *ngSwitchCase="'energy'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M13 2 4 14h6l-1 8 9-12h-6l1-8z"/></svg>
      <svg *ngSwitchCase="'science'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M9.5 3h5"/><path d="M10.3 3v6.2L4.8 19a2 2 0 0 0 1.7 3h11a2 2 0 0 0 1.7-3l-5.5-9.8V3"/><path d="M7.3 15h9.4"/></svg>
      <svg *ngSwitchCase="'transport'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><rect x="1.5" y="7" width="12.5" height="9.5" rx="1.3"/><path d="M14 10h3.5l3 3v3.5H14z"/><circle cx="6" cy="18.3" r="1.6"/><circle cx="17" cy="18.3" r="1.6"/></svg>
      <svg *ngSwitchCase="'construction'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M3 11l9-7 9 7"/><path d="M5 10v10h14V10"/><path d="M9.5 20v-6h5v6"/></svg>
      <svg *ngSwitchCase="'consulting'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><rect x="2.5" y="7.5" width="19" height="12" rx="2"/><path d="M8 7.5v-2a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/><path d="M2.5 13h19"/></svg>
      <svg *ngSwitchCase="'automotive'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M4 16.5V11l2-5h12l2 5v5.5"/><path d="M2.5 16.5h19"/><circle cx="7.5" cy="16.7" r="1.7"/><circle cx="16.5" cy="16.7" r="1.7"/></svg>
      <svg *ngSwitchCase="'agriculture'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M4 20c9 0 15-6.5 15-15.5V3h-1.5C8.5 3 4 9.5 4 18.5z"/><path d="M4 20l8-8"/></svg>
      <svg *ngSwitchCase="'telecom'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M12 22v-7.5"/><path d="M8.7 17.2a4.7 4.7 0 0 1 6.6 0"/><path d="M5.8 14.3a8.7 8.7 0 0 1 12.4 0"/><circle cx="12" cy="20.3" r="1.3"/></svg>
      <ng-container *ngSwitchDefault>{{ fallback }}</ng-container>
    </ng-container>
  `,
})
export class IndustryIconComponent {
  @Input() name = "";
  @Input() fallback = "";
}
