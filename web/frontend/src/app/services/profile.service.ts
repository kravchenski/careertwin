import { Injectable } from "@angular/core";
import { HttpClient } from "@angular/common/http";
import { Observable, of } from "rxjs";
import { catchError } from "rxjs/operators";
import { resolveProfileName } from "../utils/profile-name";

export interface ExperienceItem {
  id: string;
  role: string;
  company: string;
  start: string;
  end: string;
  current: boolean;
  description: string;
}

export interface EducationItem {
  id: string;
  school: string;
  degree: string;
  field: string;
  start: string;
  end: string;
  description: string;
}

export interface CertificateItem {
  id: string;
  name: string;
  issuer: string;
  year: string;
  url: string;
}

export interface UserProfile {
  fullName: string;
  title: string;
  email: string;
  phone: string;
  location: string;
  linkedin: string;
  github: string;
  website: string;
  summary: string;
  skills: string[];
  languages: string[];
  experience: ExperienceItem[];
  education: EducationItem[];
  certificates: CertificateItem[];
  importedFromLinkedIn: boolean;
  updatedAt: string;
}

const STORAGE_KEY = "careertwin.profile";

@Injectable({ providedIn: "root" })
export class ProfileService {
  private readonly api = "/api/v1";

  constructor(private http: HttpClient) {}

  emptyProfile(): UserProfile {
    return {
      fullName: "",
      title: "",
      email: "",
      phone: "",
      location: "",
      linkedin: "",
      github: "",
      website: "",
      summary: "",
      skills: [],
      languages: [],
      experience: [],
      education: [],
      certificates: [],
      importedFromLinkedIn: false,
      updatedAt: new Date().toISOString(),
    };
  }

  load(): UserProfile {
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (raw) {
        const profile = { ...this.emptyProfile(), ...JSON.parse(raw) };
        profile.fullName = resolveProfileName(profile.fullName);
        return profile;
      }
    } catch {
    }
    return this.emptyProfile();
  }

  save(profile: UserProfile): void {
    profile.updatedAt = new Date().toISOString();
    localStorage.setItem(STORAGE_KEY, JSON.stringify(profile));
  }

  clear(): void {
    localStorage.removeItem(STORAGE_KEY);
  }

  deleteBackend(username: string = "default"): Observable<any> {
    return this.http
      .delete(`${this.api}/profile/${username}`)
      .pipe(catchError(() => of({ success: false })));
  }

  uploadResume(file: File, username: string = "default"): Observable<any> {
    const formData = new FormData();
    formData.append("file", file);
    formData.append("username", username);
    return this.http
      .post(`${this.api}/profile/upload-resume`, formData)
      .pipe(
        catchError(() => of(null)),
      );
  }

  getAvailableProviders(): Observable<any> {
    return this.http
      .get(`${this.api}/providers/available`)
      .pipe(
        catchError(() => of({ providers: [] })),
      );
  }

  uid(): string {
    return Math.random().toString(36).slice(2, 10);
  }
}
