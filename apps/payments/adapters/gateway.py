from abc import ABC, abstractmethod
class PaymentGatewayAdapter(ABC):
    @abstractmethod
    def send_payment(self,amount): ...
    @abstractmethod
    def confirm_payment(self, reference): ...
    @abstractmethod
    def refund(self, reference, amount): ...

class MockPOSAdapter(PaymentGatewayAdapter):
    def send_payment(self,amount): return {'reference':'mock-ref','status':'PENDING','amount':str(amount)}
    def confirm_payment(self, reference): return {'reference':reference,'status':'COMPLETED'}
    def refund(self, reference, amount): return {'reference':reference,'status':'REFUNDED','amount':str(amount)}
