import { useTranslation } from "react-i18next";

export default function App() {
  const { t } = useTranslation();
  return (
    <div className="flex h-full items-center justify-center">
      <h1 className="text-2xl font-semibold text-neutral-200">
        {t("app.title")}
      </h1>
    </div>
  );
}
